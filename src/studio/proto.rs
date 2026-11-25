//! The subset of the ZMK Studio protobuf messages this app uses, written out for `prost` so no
//! `protoc` is needed at build time. Field numbers and types follow
//! zmkfirmware/zmk-studio-messages `proto/zmk/*.proto` at commit 6cb4c28. Variants not listed
//! here are skipped by the decoder (the oneof then reads as `None`).

#![allow(clippy::enum_variant_names)]

use prost::{Message, Oneof};

// ---- zmk.studio --------------------------------------------------------------------------

#[derive(Clone, PartialEq, Message)]
pub struct Request {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(oneof = "RequestSubsystem", tags = "3, 4, 5")]
    pub subsystem: Option<RequestSubsystem>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum RequestSubsystem {
    #[prost(message, tag = "3")]
    Core(CoreRequest),
    #[prost(message, tag = "4")]
    Behaviors(BehaviorsRequest),
    #[prost(message, tag = "5")]
    Keymap(KeymapRequest),
}

#[derive(Clone, PartialEq, Message)]
pub struct Response {
    #[prost(oneof = "ResponseType", tags = "1, 2")]
    pub r#type: Option<ResponseType>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum ResponseType {
    #[prost(message, tag = "1")]
    RequestResponse(RequestResponse),
    /// Contents ignored: lock state / unsaved-changes notifications.
    #[prost(message, tag = "2")]
    Notification(Empty),
}

#[derive(Clone, PartialEq, Message)]
pub struct Empty {}

#[derive(Clone, PartialEq, Message)]
pub struct RequestResponse {
    #[prost(uint32, tag = "1")]
    pub request_id: u32,
    #[prost(oneof = "ResponseSubsystem", tags = "2, 3, 4, 5")]
    pub subsystem: Option<ResponseSubsystem>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum ResponseSubsystem {
    #[prost(message, tag = "2")]
    Meta(MetaResponse),
    #[prost(message, tag = "3")]
    Core(CoreResponse),
    #[prost(message, tag = "4")]
    Behaviors(BehaviorsResponse),
    #[prost(message, tag = "5")]
    Keymap(KeymapResponse),
}

// ---- zmk.meta ----------------------------------------------------------------------------

#[derive(Clone, PartialEq, Message)]
pub struct MetaResponse {
    #[prost(oneof = "MetaResponseType", tags = "1, 2")]
    pub response_type: Option<MetaResponseType>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum MetaResponseType {
    #[prost(bool, tag = "1")]
    NoResponse(bool),
    /// `zmk.meta.ErrorConditions`: 0 generic, 1 unlock required, 2 RPC not found,
    /// 3 decode failed, 4 encode failed.
    #[prost(int32, tag = "2")]
    SimpleError(i32),
}

// ---- zmk.core ----------------------------------------------------------------------------

#[derive(Clone, PartialEq, Message)]
pub struct CoreRequest {
    #[prost(oneof = "CoreRequestType", tags = "1, 2")]
    pub request_type: Option<CoreRequestType>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum CoreRequestType {
    #[prost(bool, tag = "1")]
    GetDeviceInfo(bool),
    #[prost(bool, tag = "2")]
    GetLockState(bool),
}

#[derive(Clone, PartialEq, Message)]
pub struct CoreResponse {
    #[prost(oneof = "CoreResponseType", tags = "1, 2")]
    pub response_type: Option<CoreResponseType>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum CoreResponseType {
    #[prost(message, tag = "1")]
    GetDeviceInfo(GetDeviceInfoResponse),
    /// `zmk.core.LockState`: 0 locked, 1 unlocked.
    #[prost(int32, tag = "2")]
    GetLockState(i32),
}

#[derive(Clone, PartialEq, Message)]
pub struct GetDeviceInfoResponse {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(bytes = "vec", tag = "2")]
    pub serial_number: Vec<u8>,
}

// ---- zmk.behaviors -----------------------------------------------------------------------

#[derive(Clone, PartialEq, Message)]
pub struct BehaviorsRequest {
    #[prost(oneof = "BehaviorsRequestType", tags = "1, 2")]
    pub request_type: Option<BehaviorsRequestType>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum BehaviorsRequestType {
    #[prost(bool, tag = "1")]
    ListAllBehaviors(bool),
    #[prost(message, tag = "2")]
    GetBehaviorDetails(GetBehaviorDetailsRequest),
}

#[derive(Clone, PartialEq, Message)]
pub struct GetBehaviorDetailsRequest {
    #[prost(uint32, tag = "1")]
    pub behavior_id: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct BehaviorsResponse {
    #[prost(oneof = "BehaviorsResponseType", tags = "1, 2")]
    pub response_type: Option<BehaviorsResponseType>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum BehaviorsResponseType {
    #[prost(message, tag = "1")]
    ListAllBehaviors(ListAllBehaviorsResponse),
    #[prost(message, tag = "2")]
    GetBehaviorDetails(GetBehaviorDetailsResponse),
}

#[derive(Clone, PartialEq, Message)]
pub struct ListAllBehaviorsResponse {
    #[prost(uint32, repeated, tag = "1")]
    pub behaviors: Vec<u32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetBehaviorDetailsResponse {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(string, tag = "2")]
    pub display_name: String,
    #[prost(message, repeated, tag = "3")]
    pub metadata: Vec<BehaviorBindingParametersSet>,
}

#[derive(Clone, PartialEq, Message)]
pub struct BehaviorBindingParametersSet {
    #[prost(message, repeated, tag = "1")]
    pub param1: Vec<BehaviorParameterValueDescription>,
    #[prost(message, repeated, tag = "2")]
    pub param2: Vec<BehaviorParameterValueDescription>,
}

#[derive(Clone, PartialEq, Message)]
pub struct BehaviorParameterValueDescription {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(oneof = "ValueType", tags = "2, 3, 4, 5, 6")]
    pub value_type: Option<ValueType>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum ValueType {
    #[prost(message, tag = "2")]
    Nil(Empty),
    #[prost(uint32, tag = "3")]
    Constant(u32),
    #[prost(message, tag = "4")]
    Range(ValueRange),
    #[prost(message, tag = "5")]
    HidUsage(HidUsage),
    #[prost(message, tag = "6")]
    LayerId(Empty),
}

#[derive(Clone, PartialEq, Message)]
pub struct ValueRange {
    #[prost(int32, tag = "1")]
    pub min: i32,
    #[prost(int32, tag = "2")]
    pub max: i32,
}

#[derive(Clone, PartialEq, Message)]
pub struct HidUsage {
    #[prost(uint32, tag = "1")]
    pub keyboard_max: u32,
    #[prost(uint32, tag = "2")]
    pub consumer_max: u32,
}

// ---- zmk.keymap --------------------------------------------------------------------------

#[derive(Clone, PartialEq, Message)]
pub struct KeymapRequest {
    #[prost(oneof = "KeymapRequestType", tags = "1, 6")]
    pub request_type: Option<KeymapRequestType>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum KeymapRequestType {
    #[prost(bool, tag = "1")]
    GetKeymap(bool),
    #[prost(bool, tag = "6")]
    GetPhysicalLayouts(bool),
}

#[derive(Clone, PartialEq, Message)]
pub struct KeymapResponse {
    #[prost(oneof = "KeymapResponseType", tags = "1, 6")]
    pub response_type: Option<KeymapResponseType>,
}

#[derive(Clone, PartialEq, Oneof)]
pub enum KeymapResponseType {
    #[prost(message, tag = "1")]
    GetKeymap(Keymap),
    #[prost(message, tag = "6")]
    GetPhysicalLayouts(PhysicalLayouts),
}

#[derive(Clone, PartialEq, Message)]
pub struct Keymap {
    #[prost(message, repeated, tag = "1")]
    pub layers: Vec<Layer>,
    #[prost(uint32, tag = "2")]
    pub available_layers: u32,
    #[prost(uint32, tag = "3")]
    pub max_layer_name_length: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct Layer {
    #[prost(uint32, tag = "1")]
    pub id: u32,
    #[prost(string, tag = "2")]
    pub name: String,
    #[prost(message, repeated, tag = "3")]
    pub bindings: Vec<BehaviorBinding>,
}

#[derive(Clone, PartialEq, Message)]
pub struct BehaviorBinding {
    #[prost(sint32, tag = "1")]
    pub behavior_id: i32,
    #[prost(uint32, tag = "2")]
    pub param1: u32,
    #[prost(uint32, tag = "3")]
    pub param2: u32,
}

#[derive(Clone, PartialEq, Message)]
pub struct PhysicalLayouts {
    #[prost(uint32, tag = "1")]
    pub active_layout_index: u32,
    #[prost(message, repeated, tag = "2")]
    pub layouts: Vec<PhysicalLayout>,
}

#[derive(Clone, PartialEq, Message)]
pub struct PhysicalLayout {
    #[prost(string, tag = "1")]
    pub name: String,
    #[prost(message, repeated, tag = "2")]
    pub keys: Vec<KeyPhysicalAttrs>,
}

#[derive(Clone, PartialEq, Message)]
pub struct KeyPhysicalAttrs {
    #[prost(sint32, tag = "1")]
    pub width: i32,
    #[prost(sint32, tag = "2")]
    pub height: i32,
    #[prost(sint32, tag = "3")]
    pub x: i32,
    #[prost(sint32, tag = "4")]
    pub y: i32,
    #[prost(sint32, tag = "5")]
    pub r: i32,
    #[prost(sint32, tag = "6")]
    pub rx: i32,
    #[prost(sint32, tag = "7")]
    pub ry: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_wire_format() {
        // Request { request_id: 1, keymap { get_keymap: true } }
        // = field 1 varint 1; field 5 (len 2) { field 1 varint 1 }
        let r = Request {
            request_id: 1,
            subsystem: Some(RequestSubsystem::Keymap(KeymapRequest {
                request_type: Some(KeymapRequestType::GetKeymap(true)),
            })),
        };
        assert_eq!(r.encode_to_vec(), vec![0x08, 0x01, 0x2A, 0x02, 0x08, 0x01]);
    }

    #[test]
    fn sint32_fields_use_zigzag() {
        // KeyPhysicalAttrs r = -1000 -> zigzag 1999 -> varint CF 0F under field 5 (0x28).
        let k = KeyPhysicalAttrs { r: -1000, ..Default::default() };
        assert_eq!(k.encode_to_vec(), vec![0x28, 0xCF, 0x0F]);
        assert_eq!(KeyPhysicalAttrs::decode(&[0x28u8, 0xCF, 0x0F][..]).unwrap().r, -1000);
    }

    #[test]
    fn unknown_response_variants_are_skipped() {
        // KeymapResponse with field 2 (set_layer_binding enum = 0) which we don't model.
        let r = KeymapResponse::decode(&[0x10u8, 0x00][..]).unwrap();
        assert_eq!(r.response_type, None);
    }
}
