//! Protokollfreie serielle Infrastruktur. Dieser Baustein kennt lokale Ports
//! und USB-Metadaten, aber weder GRBL noch Ruida oder Anwendungszustände.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialPortInfo {
    pub name: String,
    pub kind: String,
    pub product: Option<String>,
    pub manufacturer: Option<String>,
    pub serial_number: Option<String>,
}

impl SerialPortInfo {
    pub fn label(&self) -> String {
        let detail = self
            .product
            .as_deref()
            .or(self.manufacturer.as_deref())
            .unwrap_or(&self.kind);
        format!("{} — {}", self.name, detail)
    }
}

pub fn available_ports() -> Result<Vec<SerialPortInfo>, String> {
    let mut ports = serialport::available_ports()
        .map_err(|error| format!("Serielle Anschlüsse konnten nicht gelesen werden: {error}"))?
        .into_iter()
        .map(|port| {
            let (kind, product, manufacturer, serial_number) = match port.port_type {
                serialport::SerialPortType::UsbPort(usb) => (
                    format!("USB {:04x}:{:04x}", usb.vid, usb.pid),
                    usb.product,
                    usb.manufacturer,
                    usb.serial_number,
                ),
                serialport::SerialPortType::BluetoothPort => ("Bluetooth".into(), None, None, None),
                serialport::SerialPortType::PciPort => ("PCI".into(), None, None, None),
                serialport::SerialPortType::Unknown => ("Seriell".into(), None, None, None),
            };
            SerialPortInfo {
                name: port.port_name,
                kind,
                product,
                manufacturer,
                serial_number,
            }
        })
        .collect::<Vec<_>>();
    ports.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(ports)
}
