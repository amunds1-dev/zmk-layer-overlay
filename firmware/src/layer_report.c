/*
 * Reports the active layer to the host over Raw HID (zzeneg/zmk-raw-hid).
 *
 * Protocol v1, 32-byte reports, unused bytes are zero.
 *
 * keyboard -> host (sent on every layer change, and in reply to a query):
 *   [0]     0x4C  magic ('L')
 *   [1]     0x01  protocol version
 *   [2]     index of the highest active layer (position in the keymap)
 *   [3]     id of the highest active layer (stable across ZMK Studio reorders)
 *   [4..7]  active layer bitmask, indexed by layer id, little endian
 *   [8..31] name of the highest active layer, NUL padded (may be empty)
 *
 * host -> keyboard:
 *   [0] 0x4C, [1] 0x3F ('?')  -> keyboard replies with a state report
 */

#include <string.h>

#include <zephyr/kernel.h>
#include <zephyr/logging/log.h>
#include <zephyr/sys/byteorder.h>

#include <zmk/event_manager.h>
#include <zmk/events/layer_state_changed.h>
#include <zmk/keymap.h>

#include <raw_hid/events.h>

LOG_MODULE_DECLARE(zmk, CONFIG_ZMK_LOG_LEVEL);

#define LR_MAGIC 0x4C
#define LR_VERSION 0x01
#define LR_QUERY 0x3F
#define LR_NAME_OFFSET 8
#define LR_REPORT_LEN 32

BUILD_ASSERT(CONFIG_RAW_HID_REPORT_SIZE >= LR_REPORT_LEN, "Raw HID report size too small");

static uint8_t report[LR_REPORT_LEN];

static void send_state(struct k_work *work) {
    zmk_keymap_layers_state_t state = zmk_keymap_layer_state();
    zmk_keymap_layer_index_t index = zmk_keymap_highest_layer_active();
    zmk_keymap_layer_id_t id = zmk_keymap_layer_index_to_id(index);
    const char *name = zmk_keymap_layer_name(id);

    memset(report, 0, sizeof(report));
    report[0] = LR_MAGIC;
    report[1] = LR_VERSION;
    report[2] = index;
    report[3] = id;
    sys_put_le32(state, &report[4]);
    if (name != NULL) {
        strncpy((char *)&report[LR_NAME_OFFSET], name, LR_REPORT_LEN - LR_NAME_OFFSET - 1);
    }

    raise_raw_hid_sent_event(
        (struct raw_hid_sent_event){.data = report, .length = sizeof(report)});
}

/* Sending is deferred to the system work queue: layer events and Raw HID
 * receive callbacks can arrive from contexts where blocking is not allowed. */
static K_WORK_DEFINE(send_work, send_state);

static int layer_report_listener(const zmk_event_t *eh) {
    if (as_zmk_layer_state_changed(eh) != NULL) {
        k_work_submit(&send_work);
        return ZMK_EV_EVENT_BUBBLE;
    }

    const struct raw_hid_received_event *rx = as_raw_hid_received_event(eh);
    if (rx != NULL && rx->length >= 2 && rx->data[0] == LR_MAGIC && rx->data[1] == LR_QUERY) {
        k_work_submit(&send_work);
    }

    return ZMK_EV_EVENT_BUBBLE;
}

ZMK_LISTENER(layer_report, layer_report_listener);
ZMK_SUBSCRIPTION(layer_report, zmk_layer_state_changed);
ZMK_SUBSCRIPTION(layer_report, raw_hid_received_event);
