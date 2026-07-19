//! HW-verifizierte Ruida-Maschinenregister. Protokolladressen und Raw-Werte
//! bleiben vollständig im Ruida-Treiber.

use crate::protocol::cmd_write_reg;
use crate::{read_reg_optional, to_driver_err, RuidaDriver};
use luxifer_core::{DriverError, MachineSetting, MachineSettingUnit};

#[derive(Debug, Clone, Copy)]
pub struct RuidaSettingDef {
    pub address: u16,
    pub key: &'static str,
    pub label: &'static str,
    pub group: &'static str,
    pub unit: MachineSettingUnit,
    pub bit_mask: Option<i64>,
    pub options: &'static [(i64, &'static str)],
    pub mirror: Option<u16>,
}

fn setting_from_def(def: &RuidaSettingDef, raw: Option<i64>) -> MachineSetting {
    MachineSetting {
        address: def.address,
        key: def.key.into(),
        label: def.label.into(),
        group: def.group.into(),
        unit: def.unit,
        curated: true,
        writable: !PROTECTED.contains(&def.address),
        bit_mask: def.bit_mask,
        options: def.options.iter().map(|(v, l)| (*v, (*l).into())).collect(),
        raw,
        mirror: def.mirror,
    }
}

const NONE: &[(i64, &str)] = &[];
const ON_OFF: &[(i64, &str)] = &[(0, "Aus"), (1, "Ein")];
const ENGRAVING: &[(i64, &str)] = &[(0, "Common Mode"), (0x0400, "Special Mode")];
macro_rules! s {
    ($a:expr,$k:expr,$l:expr,$u:ident,$g:expr) => {
        RuidaSettingDef {
            address: $a,
            key: $k,
            label: $l,
            group: $g,
            unit: MachineSettingUnit::$u,
            bit_mask: None,
            options: NONE,
            mirror: None,
        }
    };
}

pub static SETTINGS: &[RuidaSettingDef] = &[
    s!(
        0x0005,
        "idle_speed",
        "Idle speed",
        MmPerSec,
        "Schnittparameter"
    ),
    s!(
        0x020A,
        "idle_accel",
        "Idle acceleration",
        MmPerSec2,
        "Schnittparameter"
    ),
    s!(0x0203, "idle_delay", "Idle delay", Raw, "Schnittparameter"),
    s!(
        0x0201,
        "start_speed",
        "Start speed",
        MmPerSec,
        "Schnittparameter"
    ),
    s!(
        0x0209,
        "min_accel",
        "Min acceleration",
        MmPerSec2,
        "Schnittparameter"
    ),
    s!(
        0x0202,
        "max_accel",
        "Max acceleration",
        MmPerSec2,
        "Schnittparameter"
    ),
    s!(
        0x021A,
        "accel_factor",
        "Accel factor",
        Percent,
        "Schnittparameter"
    ),
    s!(
        0x021C,
        "g0_accel_factor",
        "G0 accel factor",
        Percent,
        "Schnittparameter"
    ),
    s!(
        0x021B,
        "speed_factor",
        "Speed factor",
        Percent,
        "Schnittparameter"
    ),
    s!(
        0x0224,
        "eng_x_start_speed",
        "X start speed",
        MmPerSec,
        "Gravurparameter"
    ),
    s!(
        0x0234,
        "eng_y_start_speed",
        "Y start speed",
        MmPerSec,
        "Gravurparameter"
    ),
    s!(
        0x0225,
        "eng_x_accel",
        "X acceleration",
        MmPerSec2,
        "Gravurparameter"
    ),
    s!(
        0x0235,
        "eng_y_accel",
        "Y acceleration",
        MmPerSec2,
        "Gravurparameter"
    ),
    s!(
        0x000E,
        "line_shift_speed",
        "Line shift speed",
        MmPerSec,
        "Gravurparameter"
    ),
    s!(
        0x000B,
        "facula_size",
        "Facula size",
        PermillePercent,
        "Gravurparameter"
    ),
    s!(
        0x0237,
        "eng_factor",
        "Engraving factor",
        Percent,
        "Gravurparameter"
    ),
    RuidaSettingDef {
        address: 0x0010,
        key: "engraving_mode",
        label: "Engraving mode",
        group: "Gravurparameter",
        unit: MachineSettingUnit::Enum,
        bit_mask: Some(0x0400),
        options: ENGRAVING,
        mirror: None,
    },
    s!(
        0x000C,
        "reset_search_speed",
        "Reset Suchfahrt",
        MmPerSec,
        "Referenzfahrt"
    ),
    RuidaSettingDef {
        mirror: Some(0x0241),
        ..s!(
            0x0240,
            "reset_touch_speed",
            "Reset Feintasten",
            MmPerSec,
            "Referenzfahrt"
        )
    },
    s!(
        0x0242,
        "reset_retract_speed",
        "Reset Freifahren",
        MmPerSec,
        "Referenzfahrt"
    ),
    RuidaSettingDef {
        address: 0x0226,
        key: "rotary_enable",
        label: "Rotary aktiv",
        group: "Rotary",
        unit: MachineSettingUnit::Enum,
        bit_mask: Some(1),
        options: ON_OFF,
        mirror: None,
    },
    s!(
        0x021F,
        "pulses_per_rot",
        "Pulse pro Umdrehung",
        Pulse,
        "Rotary"
    ),
    s!(0x0221, "rotary_diameter", "Durchmesser", Mm, "Rotary"),
    s!(
        0x0021,
        "x_step_length",
        "X Step Length",
        StepLength,
        "Achsenkalibrierung"
    ),
    s!(
        0x0031,
        "y_step_length",
        "Y Step Length",
        StepLength,
        "Achsenkalibrierung"
    ),
    s!(
        0x0041,
        "z_step_length",
        "Z Step Length",
        StepLength,
        "Achsenkalibrierung"
    ),
    s!(
        0x0051,
        "u_step_length",
        "U/E Step Length",
        StepLength,
        "Achsenkalibrierung"
    ),
    s!(0x020E, "focus_distance", "Focus distance", Mm, "Sonstiges"),
    s!(
        0x0231,
        "wl_speed_fast",
        "Wireless speed fast",
        MmPerSec,
        "Sonstiges"
    ),
    s!(
        0x0232,
        "wl_speed_slow",
        "Wireless speed slow",
        MmPerSec,
        "Sonstiges"
    ),
];

const RAW: &[u16] = &[
    0x0011, 0x0017, 0x001A, 0x001C, 0x0027, 0x0028, 0x0037, 0x0038, 0x0046, 0x0047, 0x0048, 0x0056,
    0x0057, 0x0058, 0x0200, 0x0215, 0x0216, 0x0238, 0x0243, 0x030F, 0x0351, 0x0020, 0x0023, 0x0024,
    0x0025, 0x0026, 0x0030, 0x0033, 0x0034, 0x0035, 0x0036, 0x0040, 0x0043, 0x0044, 0x0045, 0x0050,
    0x0053, 0x0054, 0x0055,
];
const PROTECTED: &[u16] = &[0x0207, 0x020B, 0x0400, 0x057E, 0x0004, 0x0005];

impl RuidaDriver {
    pub fn read_machine_settings(&self) -> Result<Vec<MachineSetting>, DriverError> {
        let t = self.transport()?;
        let mut out = Vec::new();
        for def in SETTINGS {
            out.push(setting_from_def(def, read_reg_optional(t, def.address)?));
        }
        for &address in RAW {
            out.push(MachineSetting {
                address,
                key: format!("mem_{address:04x}"),
                label: format!("0x{address:04X}"),
                group: "Raw-Register".into(),
                unit: MachineSettingUnit::Raw,
                curated: false,
                writable: !PROTECTED.contains(&address),
                bit_mask: None,
                options: Vec::new(),
                raw: read_reg_optional(t, address)?,
                mirror: None,
            });
        }
        Ok(out)
    }

    pub fn write_machine_settings(&self, changes: &[(u16, i64)]) -> Result<(), DriverError> {
        let t = self.transport()?;
        for &(address, raw) in changes {
            if PROTECTED.contains(&address) {
                return Err(DriverError::Transport(format!(
                    "Register 0x{address:04X} ist geschützt"
                )));
            }
            t.send(&cmd_write_reg(address, raw))
                .map_err(to_driver_err)?;
        }
        for address in [0x0207, 0x020B] {
            t.send(&cmd_write_reg(address, 0)).map_err(to_driver_err)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_enthaelt_kuratierte_achsen_und_raw_profile() {
        assert!(SETTINGS
            .iter()
            .any(|s| s.key == "z_step_length" && s.address == 0x0041));
        assert!(SETTINGS
            .iter()
            .any(|s| s.key == "u_step_length" && s.address == 0x0051));
        assert!(RAW.contains(&0x0040));
        assert!(RAW.contains(&0x0050));
    }

    #[test]
    fn einheiten_und_bitmasken_erhalten_rohwerte() {
        assert_eq!(MachineSettingUnit::MmPerSec.factor(), 1000.0);
        let engraving = SETTINGS.iter().find(|s| s.key == "engraving_mode").unwrap();
        let old = 0x2000_i64;
        let mask = engraving.bit_mask.unwrap();
        assert_eq!((old & !mask) | (0x0400 & mask), 0x2400);
    }
}
