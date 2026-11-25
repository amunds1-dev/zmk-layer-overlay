/*
 * Reports the active layer and pressed keys to the host over Raw HID
 * (zzeneg/zmk-raw-hid), for the on-screen overlay app.
 *
 * 32-byte reports, unused bytes are zero. Byte 0 is always the magic 0x4C ('L'),
 * byte 1 is the report type.
 *
 * keyboard -> host, type 0x01: layer state (on every layer change, and on query)
 *   [2]     index of the highest active layer (position in the keymap)
 *   [3]     id of the highest active layer (stable across ZMK Studio reorders)
 *   [4..7]  active layer bitmask, indexed by layer id, little endian
 *   [8..31] name of the highest active layer, NUL padded (may be empty)
 *
 * keyboard -> host, type 0x02: pressed keys (on every key press/release, and on query)
 *   [2..9]  bitmask of pressed key positions 0..63, little endian. Includes keys
 *           on the peripheral half. Sent as a full state so a lost report can't
 *           leave a key stuck.
 *
 * host -> keyboard:
 *   [0] 0x4C, [1] 0x3F ('?')  -> keyboard replies with both report types
 */

#include <string.h>

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/sys/byteorder.h>

#include <zmk/event_manager.h>
#include <zmk/events/layer_state_changed.h>
#include <zmk/keymap.h>

#if IS_ENABLED(CONFIG_LAYER_OVERLAY_KEY_REPORT)
#include <zmk/events/position_state_changed.h>
#endif

#include <raw_hid/events.h>

LOG_MODULE_DECLARE(zmk, CONFIG_ZMK_LOG_LEVEL);

#define LR_MAGIC 0x4C
#define LR_TYPE_LAYER 0x01
#define LR_TYPE_KEYS 0x02
#define LR_QUERY 0x3F
#define LR_NAME_OFFSET 8
#define LR_REPORT_LEN 32

BUILD_ASSERT(CONFIG_RAW_HID_REPORT_SIZE >= LR_REPORT_LEN, "Raw HID report size too small");

/* Raw HID listeners copy the data synchronously, so one buffer can be reused. Both
 * work items run on the system work queue, which serialises them. */
static uint8_t report[LR_REPORT_LEN];

static void send_report(void) {
    raise_raw_hid_sent_event(
        (struct raw_hid_sent_event){.data = report, .length = sizeof(report)});
}

static void send_layer_state(struct k_work *work) {
    zmk_keymap_layers_state_t state = zmk_keymap_layer_state();
    zmk_keymap_layer_index_t index = zmk_keymap_highest_layer_active();
    zmk_keymap_layer_id_t id = zmk_keymap_layer_index_to_id(index);
    const char *name = zmk_keymap_layer_name(id);

    memset(report, 0, sizeof(report));
    report[0] = LR_MAGIC;
    report[1] = LR_TYPE_LAYER;
    report[2] = index;
    report[3] = id;
    sys_put_le32(state, &report[4]);
    if (name != NULL) {
        strncpy((char *)&report[LR_NAME_OFFSET], name, LR_REPORT_LEN - LR_NAME_OFFSET - 1);
    }
    send_report();
}

/* Sending is deferred to the system work queue: events can arrive from contexts
 * where blocking is not allowed, and bursts of key events coalesce into one report. */
static K_WORK_DEFINE(layer_work, send_layer_state);

#if IS_ENABLED(CONFIG_LAYER_OVERLAY_KEY_REPORT)
static atomic_t pressed_lo; /* positions 0..31 */
static atomic_t pressed_hi; /* positions 32..63 */

static void send_key_state(struct k_work *work) {
    memset(report, 0, sizeof(report));
    report[0] = LR_MAGIC;
    report[1] = LR_TYPE_KEYS;
    sys_put_le32((uint32_t)atomic_get(&pressed_lo), &report[2]);
    sys_put_le32((uint32_t)atomic_get(&pressed_hi), &report[6]);
    send_report();
}

static K_WORK_DEFINE(key_work, send_key_state);

static void track_position(const struct zmk_position_state_changed *ev) {
    if (ev->position >= 64) {
        return;
    }
    atomic_t *word = ev->position < 32 ? &pressed_lo : &pressed_hi;
    int bit = ev->position % 32;
    if (ev->state) {
        atomic_set_bit(word, bit);
    } else {
        atomic_clear_bit(word, bit);
    }
    k_work_submit(&key_work);
}
#endif

static int layer_report_listener(const zmk_event_t *eh) {
    if (as_zmk_layer_state_changed(eh) != NULL) {
        k_work_submit(&layer_work);
        return ZMK_EV_EVENT_BUBBLE;
    }

#if IS_ENABLED(CONFIG_LAYER_OVERLAY_KEY_REPORT)
    const struct zmk_position_state_changed *pos = as_zmk_position_state_changed(eh);
    if (pos != NULL) {
        track_position(pos);
        return ZMK_EV_EVENT_BUBBLE;
    }
#endif

    const struct raw_hid_received_event *rx = as_raw_hid_received_event(eh);
    if (rx != NULL && rx->length >= 2 && rx->data[0] == LR_MAGIC && rx->data[1] == LR_QUERY) {
        k_work_submit(&layer_work);
#if IS_ENABLED(CONFIG_LAYER_OVERLAY_KEY_REPORT)
        k_work_submit(&key_work);
#endif
    }

    return ZMK_EV_EVENT_BUBBLE;
}

ZMK_LISTENER(layer_report, layer_report_listener);
ZMK_SUBSCRIPTION(layer_report, zmk_layer_state_changed);
ZMK_SUBSCRIPTION(layer_report, raw_hid_received_event);
#if IS_ENABLED(CONFIG_LAYER_OVERLAY_KEY_REPORT)
ZMK_SUBSCRIPTION(layer_report, zmk_position_state_changed);
#endif
