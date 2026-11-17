//! A small Bluetooth LE GATT server: an attribute table and the Attribute Protocol (ATT)
//! requests a client sends to it.
//!
//! Constants as defined in Bluetooth Core Vol 3 Part F (ATT) and Part G (GATT), taken from
//! the BlueZ headers `src/shared/att-types.h` and `lib/bluetooth/uuid.h`. All multi-byte
//! values are little-endian on the air.

// ATT opcodes.
pub const OP_ERROR_RSP: u8 = 0x01;
pub const OP_READ_REQ: u8 = 0x0A;
pub const OP_READ_RSP: u8 = 0x0B;
pub const OP_WRITE_REQ: u8 = 0x12;
pub const OP_WRITE_RSP: u8 = 0x13;
pub const OP_HANDLE_NFY: u8 = 0x1B;

// ATT error codes.
pub const ERR_INVALID_HANDLE: u8 = 0x01;
pub const ERR_READ_NOT_PERMITTED: u8 = 0x02;
pub const ERR_WRITE_NOT_PERMITTED: u8 = 0x03;

// Attribute types (16-bit UUIDs).
pub const UUID_PRIMARY_SERVICE: u16 = 0x2800;
pub const UUID_CHARACTERISTIC: u16 = 0x2803;
pub const UUID_CCCD: u16 = 0x2902;
pub const UUID_GAP_SERVICE: u16 = 0x1800;
pub const UUID_DEVICE_NAME: u16 = 0x2A00;
pub const UUID_BATTERY_SERVICE: u16 = 0x180F;
pub const UUID_BATTERY_LEVEL: u16 = 0x2A19;

// Characteristic properties (first byte of a characteristic declaration).
pub const PROP_READ: u8 = 0x02;
pub const PROP_WRITE: u8 = 0x08;
pub const PROP_NOTIFY: u8 = 0x10;

/// The default ATT MTU on LE: a response carries at most MTU - 1 bytes of value.
pub const DEFAULT_MTU: usize = 23;

/// Client Characteristic Configuration bits: notifications 0x0001, indications 0x0002.
pub const CCCD_NOTIFY: u16 = 0x0001;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub handle: u16,
    pub type_uuid: u16,
    pub value: Vec<u8>,
    pub readable: bool,
    pub writable: bool,
}

/// What a 16-bit attribute type means, in words.
pub fn uuid_name(uuid: u16) -> &'static str {
    match uuid {
        UUID_PRIMARY_SERVICE => "Primary Service declaration",
        UUID_CHARACTERISTIC => "Characteristic declaration",
        UUID_CCCD => "Client Characteristic Configuration",
        UUID_GAP_SERVICE => "Generic Access service",
        UUID_DEVICE_NAME => "Device Name",
        UUID_BATTERY_SERVICE => "Battery service",
        UUID_BATTERY_LEVEL => "Battery Level",
        _ => "unknown",
    }
}

fn le16(v: u16) -> [u8; 2] {
    v.to_le_bytes()
}

/// A characteristic declaration's value: properties, the handle of the value, its type.
pub fn characteristic_declaration(props: u8, value_handle: u16, uuid: u16) -> Vec<u8> {
    let mut v = vec![props];
    v.extend(le16(value_handle));
    v.extend(le16(uuid));
    v
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GattServer {
    pub attrs: Vec<Attribute>,
}

impl GattServer {
    /// A small sensor: the mandatory Generic Access service with the device's name, and a
    /// Battery service whose level can be read and notified.
    pub fn sensor(name: &str, battery_percent: u8) -> Self {
        let a = |handle, type_uuid, value: Vec<u8>, writable| Attribute { handle, type_uuid, value, readable: true, writable };
        let mut battery_level = a(0x0006, UUID_BATTERY_LEVEL, vec![battery_percent], false);
        battery_level.readable = true;
        Self {
            attrs: vec![
                a(0x0001, UUID_PRIMARY_SERVICE, le16(UUID_GAP_SERVICE).to_vec(), false),
                a(0x0002, UUID_CHARACTERISTIC, characteristic_declaration(PROP_READ, 0x0003, UUID_DEVICE_NAME), false),
                a(0x0003, UUID_DEVICE_NAME, name.as_bytes().to_vec(), false),
                a(0x0004, UUID_PRIMARY_SERVICE, le16(UUID_BATTERY_SERVICE).to_vec(), false),
                a(0x0005, UUID_CHARACTERISTIC, characteristic_declaration(PROP_READ | PROP_NOTIFY, 0x0006, UUID_BATTERY_LEVEL), false),
                battery_level,
                a(0x0007, UUID_CCCD, vec![0x00, 0x00], true),
            ],
        }
    }

    pub fn get(&self, handle: u16) -> Option<&Attribute> {
        self.attrs.iter().find(|a| a.handle == handle)
    }

    fn error(request_opcode: u8, handle: u16, code: u8) -> Vec<u8> {
        let mut v = vec![OP_ERROR_RSP, request_opcode];
        v.extend(le16(handle));
        v.push(code);
        v
    }

    /// Answer one request PDU. Error Response layout: opcode 0x01, the opcode of the
    /// request that failed, the handle concerned, an error code.
    pub fn handle_request(&mut self, pdu: &[u8]) -> Vec<u8> {
        let (Some(&op), Some(h)) = (pdu.first(), pdu.get(1..3).map(|b| u16::from_le_bytes([b[0], b[1]]))) else {
            return Self::error(pdu.first().copied().unwrap_or(0), 0, ERR_INVALID_HANDLE);
        };
        let Some(i) = self.attrs.iter().position(|a| a.handle == h) else {
            return Self::error(op, h, ERR_INVALID_HANDLE);
        };
        match op {
            OP_READ_REQ if !self.attrs[i].readable => Self::error(op, h, ERR_READ_NOT_PERMITTED),
            OP_READ_REQ => std::iter::once(OP_READ_RSP).chain(self.attrs[i].value.iter().copied().take(DEFAULT_MTU - 1)).collect(),
            OP_WRITE_REQ if !self.attrs[i].writable => Self::error(op, h, ERR_WRITE_NOT_PERMITTED),
            OP_WRITE_REQ => {
                self.attrs[i].value = pdu[3..].to_vec();
                vec![OP_WRITE_RSP]
            }
            _ => Self::error(op, h, ERR_INVALID_HANDLE),
        }
    }

    /// The handle of the configuration descriptor that belongs to a characteristic value:
    /// the CCCD follows its value directly in this table.
    pub fn cccd_of(&self, value_handle: u16) -> Option<u16> {
        self.get(value_handle + 1).filter(|a| a.type_uuid == UUID_CCCD).map(|a| a.handle)
    }

    pub fn notifications_on(&self, value_handle: u16) -> bool {
        self.cccd_of(value_handle)
            .and_then(|c| self.get(c))
            .is_some_and(|a| a.value.len() >= 2 && u16::from_le_bytes([a.value[0], a.value[1]]) & CCCD_NOTIFY != 0)
    }

    /// The server changes a value. If the client has switched notifications on, the
    /// server sends a Handle Value Notification: opcode 0x1B, the handle, the new value.
    pub fn update(&mut self, value_handle: u16, value: Vec<u8>) -> Option<Vec<u8>> {
        let a = self.attrs.iter_mut().find(|a| a.handle == value_handle)?;
        a.value = value.clone();
        self.notifications_on(value_handle).then(|| {
            let mut v = vec![OP_HANDLE_NFY];
            v.extend(le16(value_handle));
            v.extend(value);
            v
        })
    }
}

/// A Read Request for `handle`.
pub fn read_request(handle: u16) -> Vec<u8> {
    let mut v = vec![OP_READ_REQ];
    v.extend(le16(handle));
    v
}

/// A Write Request: `handle` followed by the new value.
pub fn write_request(handle: u16, value: &[u8]) -> Vec<u8> {
    let mut v = vec![OP_WRITE_REQ];
    v.extend(le16(handle));
    v.extend(value);
    v
}

/// The fields of an ATT PDU, for display: `(bytes, meaning)`.
pub fn describe_pdu(pdu: &[u8]) -> Vec<(Vec<u8>, String)> {
    let Some(&op) = pdu.first() else { return Vec::new() };
    let handle = |b: &[u8]| format!("handle 0x{:04X} (little-endian)", u16::from_le_bytes([b[0], b[1]]));
    let mut out = Vec::new();
    let name = match op {
        OP_ERROR_RSP => "Error Response",
        OP_READ_REQ => "Read Request",
        OP_READ_RSP => "Read Response",
        OP_WRITE_REQ => "Write Request",
        OP_WRITE_RSP => "Write Response",
        OP_HANDLE_NFY => "Handle Value Notification",
        _ => "unknown opcode",
    };
    out.push((vec![op], format!("opcode 0x{op:02X}: {name}")));
    match op {
        OP_ERROR_RSP if pdu.len() >= 5 => {
            out.push((vec![pdu[1]], format!("the request that failed: 0x{:02X}", pdu[1])));
            out.push((pdu[2..4].to_vec(), handle(&pdu[2..4])));
            let why = match pdu[4] {
                ERR_INVALID_HANDLE => "Invalid Handle: no attribute has this handle",
                ERR_READ_NOT_PERMITTED => "Read Not Permitted",
                ERR_WRITE_NOT_PERMITTED => "Write Not Permitted",
                _ => "other error",
            };
            out.push((vec![pdu[4]], format!("error code 0x{:02X}: {why}", pdu[4])));
        }
        OP_READ_REQ if pdu.len() >= 3 => out.push((pdu[1..3].to_vec(), handle(&pdu[1..3]))),
        OP_WRITE_REQ | OP_HANDLE_NFY if pdu.len() >= 3 => {
            out.push((pdu[1..3].to_vec(), handle(&pdu[1..3])));
            if pdu.len() > 3 {
                out.push((pdu[3..].to_vec(), "the value".to_owned()));
            }
        }
        OP_READ_RSP if pdu.len() > 1 => out.push((pdu[1..].to_vec(), "the value".to_owned())),
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_table_layout() {
        let s = GattServer::sensor("Sensor", 90);
        assert_eq!(s.get(0x0004).unwrap().value, [0x0F, 0x18], "0x180F, little-endian");
        // Read and notify (0x12), value at handle 0x0006, type 0x2A19.
        assert_eq!(s.get(0x0005).unwrap().value, [0x12, 0x06, 0x00, 0x19, 0x2A]);
        assert_eq!(s.cccd_of(0x0006), Some(0x0007));
        assert_eq!(s.cccd_of(0x0003), None);
    }

    #[test]
    fn reads_writes_and_errors() {
        let mut s = GattServer::sensor("Sensor", 90);
        assert_eq!(read_request(0x0003), [0x0A, 0x03, 0x00]);
        let mut name = vec![0x0B];
        name.extend(b"Sensor");
        assert_eq!(s.handle_request(&read_request(0x0003)), name);
        assert_eq!(s.handle_request(&read_request(0x0006)), [0x0B, 90]);
        // No such handle.
        assert_eq!(s.handle_request(&read_request(0x0020)), [0x01, 0x0A, 0x20, 0x00, 0x01]);
        // The battery level cannot be written.
        assert_eq!(s.handle_request(&write_request(0x0006, &[50])), [0x01, 0x12, 0x06, 0x00, 0x03]);
        assert_eq!(s.get(0x0006).unwrap().value, [90]);
        // A long name is cut to MTU - 1 = 22 bytes in a Read Response.
        let mut long = GattServer::sensor("A very long device name here", 90);
        assert_eq!(long.handle_request(&read_request(0x0003)).len(), 23);
    }

    #[test]
    fn notifications_need_the_client_to_ask() {
        let mut s = GattServer::sensor("Sensor", 90);
        assert_eq!(s.update(0x0006, vec![89]), None, "not switched on yet");
        assert_eq!(write_request(0x0007, &[0x01, 0x00]), [0x12, 0x07, 0x00, 0x01, 0x00]);
        assert_eq!(s.handle_request(&write_request(0x0007, &[0x01, 0x00])), [0x13]);
        assert_eq!(s.update(0x0006, vec![88]), Some(vec![0x1B, 0x06, 0x00, 88]));
        // Off again.
        s.handle_request(&write_request(0x0007, &[0x00, 0x00]));
        assert_eq!(s.update(0x0006, vec![87]), None);
        let d = describe_pdu(&[0x01, 0x12, 0x06, 0x00, 0x03]);
        assert_eq!(d.len(), 4);
        assert!(d[3].1.contains("Write Not Permitted"));
    }
}
