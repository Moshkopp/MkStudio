//! Ruida RDC6445G — Protokoll-Kodierung (Swizzle, Werte, Befehle).
//!
//! Neu implementiert nach der ThorBurn-Referenz (`hardware/protocol.rs`), deren
//! Werte an echter Hardware verifiziert sind. Reine Datentransformation, keine
//! I/O.

/// Magic-Byte für die RDC6445G-Verschlüsselung.
pub const MAGIC: u8 = 0x88;

// --- Swizzle ----------------------------------------------------------------

/// Verschlüsselt ein Byte (Ruida-„Scramble").
pub fn swizzle_byte(mut b: u8, magic: u8) -> u8 {
    b ^= b >> 7;
    b ^= b << 7;
    b ^= b >> 7;
    b ^= magic;
    b.wrapping_add(1)
}

/// Kehrt `swizzle_byte` um.
pub fn unswizzle_byte(mut b: u8, magic: u8) -> u8 {
    b = b.wrapping_sub(1);
    b ^= magic;
    b ^= b >> 7;
    b ^= b << 7;
    b ^= b >> 7;
    b
}

pub fn swizzle(data: &[u8], magic: u8) -> Vec<u8> {
    data.iter().map(|&b| swizzle_byte(b, magic)).collect()
}

pub fn unswizzle(data: &[u8], magic: u8) -> Vec<u8> {
    data.iter().map(|&b| unswizzle_byte(b, magic)).collect()
}

// --- Zahlen-Kodierung -------------------------------------------------------

/// 7-Bit-pro-Byte, big-endian, `length` Bytes.
pub fn encode_value(mut value: u64, length: usize) -> Vec<u8> {
    let mut out = vec![0u8; length];
    for i in (0..length).rev() {
        out[i] = (value & 0x7F) as u8;
        value >>= 7;
    }
    out
}

/// 5-Byte-Koordinate in µm (35-Bit-Maske für Zweierkomplement).
pub fn encode_coord(um: i32) -> Vec<u8> {
    let mask: i64 = (1 << 35) - 1;
    encode_value((um as i64 & mask) as u64, 5)
}

/// Leistung 0–100 % → 14-Bit-Wert (2 Byte).
pub fn encode_power(pct: f64) -> Vec<u8> {
    encode_value((pct / 100.0 * 0x3FFF as f64).round() as u64, 2)
}

/// Speed in mm/s → 5-Byte µm/s.
pub fn encode_speed(mm_s: f64) -> Vec<u8> {
    encode_value((mm_s * 1000.0) as u64, 5)
}

// --- Paket-Aufbau -----------------------------------------------------------

/// 16-Bit-Checksum über die bereits geswizzelten Bytes, big-endian.
pub fn checksum(scrambled: &[u8]) -> [u8; 2] {
    let s = scrambled.iter().map(|&b| b as u32).sum::<u32>() & 0xFFFF;
    (s as u16).to_be_bytes()
}

/// Fertiges UDP-Paket: `[2 Byte Checksum][geswizzelte Nutzdaten]`.
pub fn build_packet(payload: &[u8], magic: u8) -> Vec<u8> {
    let scrambled = swizzle(payload, magic);
    let cs = checksum(&scrambled);
    let mut pkt = Vec::with_capacity(2 + scrambled.len());
    pkt.extend_from_slice(&cs);
    pkt.extend_from_slice(&scrambled);
    pkt
}

// --- Befehle ----------------------------------------------------------------

/// Vorschubgeschwindigkeit setzen (`C9 02`).
pub fn cmd_set_speed(mm_s: f64) -> Vec<u8> {
    let mut v = vec![0xC9, 0x02];
    v.extend(encode_speed(mm_s));
    v
}

/// Laser-Leistung Layer-Ebene 0 setzen (`C6 01` min, `C6 02` max — vereinfacht).
pub fn cmd_set_power_max(pct: f64) -> Vec<u8> {
    let mut v = vec![0xC6, 0x02];
    v.extend(encode_power(pct));
    v
}

pub fn cmd_set_power_min(pct: f64) -> Vec<u8> {
    let mut v = vec![0xC6, 0x01];
    v.extend(encode_power(pct));
    v
}

/// Absolut fahren, Laser AUS (`88`).
pub fn cmd_move_abs(x_um: i32, y_um: i32) -> Vec<u8> {
    let mut v = vec![0x88];
    v.extend(encode_coord(x_um));
    v.extend(encode_coord(y_um));
    v
}

/// Absolut schneiden, Laser AN (`A8`).
pub fn cmd_cut_abs(x_um: i32, y_um: i32) -> Vec<u8> {
    let mut v = vec![0xA8];
    v.extend(encode_coord(x_um));
    v.extend(encode_coord(y_um));
    v
}

/// Bewegung stoppen (`D8 01`).
pub fn cmd_stop() -> Vec<u8> {
    vec![0xD8, 0x01]
}

/// Laufenden Prozess pausieren/fortsetzen (`D8 02`, Referenzprotokoll).
pub fn cmd_pause() -> Vec<u8> {
    vec![0xD8, 0x02]
}

/// Eilgang (Rapid) absolut, Laser AUS (`D9 10 00`) — für Jog/Home/Frame.
pub fn cmd_rapid_move_xy(x_um: i32, y_um: i32) -> Vec<u8> {
    let mut v = vec![0xD9, 0x10, 0x00];
    v.extend(encode_coord(x_um));
    v.extend(encode_coord(y_um));
    v
}

// --- Register-Abfrage (Status/Position) -------------------------------------

pub const ADDR_STATUS: u16 = 0x0400;
pub const ADDR_POS_X: u16 = 0x0421;
pub const ADDR_POS_Y: u16 = 0x0431;
/// Benutzerursprung (am Panel gesetzt), an HW verifiziert (gotoorigin.pcap).
pub const ADDR_ORIGIN_X: u16 = 0x0424;
pub const ADDR_ORIGIN_Y: u16 = 0x0434;

/// Register lesen (`DA 00 <hi> <lo>`). Antwort: `DA 01 <hi> <lo> <5-Byte-Wert>`.
pub fn cmd_read_reg(addr: u16) -> Vec<u8> {
    vec![0xDA, 0x00, (addr >> 8) as u8, (addr & 0xFF) as u8]
}

/// 7-Bit-pro-Byte big-endian dekodieren (Umkehrung von [`encode_value`]).
pub fn decode_value(data: &[u8]) -> u64 {
    data.iter()
        .fold(0u64, |acc, &b| (acc << 7) | (b & 0x7F) as u64)
}

/// 5-Byte-Koordinate (µm) als signed 32-Bit dekodieren.
pub fn decode_coord(data: &[u8]) -> i32 {
    let v = decode_value(&data[..5.min(data.len())]) & 0xFFFF_FFFF;
    if v > 0x7FFF_FFFF {
        (v as i64 - 0x1_0000_0000) as i32
    } else {
        v as i32
    }
}

/// mm → µm (ganzzahlig gerundet).
pub fn mm_to_um(mm: f64) -> i32 {
    (mm * 1000.0).round() as i32
}

// --- Job-Rahmen (Preamble/Trailer, HW-verifizierte Konstanten) --------------

/// Dateiende-Byte (Job endet damit).
pub const END_OF_FILE: u8 = 0xD7;
/// Opcode „Dateisumme setzen".
pub const SET_FILE_SUM: [u8; 2] = [0xE5, 0x05];
/// Antwort-Bytes des Controllers.
pub const ACK: u8 = 0xCC;
pub const NAK: u8 = 0xCF;
pub const ERR: u8 = 0xCD;

/// Trailer mit Dateisumme über den gesamten bisherigen Job.
pub fn recompute_file_sum(job: &[u8]) -> Vec<u8> {
    let sum = (job.iter().map(|&b| b as u64).sum::<u64>() + END_OF_FILE as u64) & 0xFFFF_FFFF;
    let mut trailer = SET_FILE_SUM.to_vec();
    trailer.extend(encode_value(sum, 5));
    trailer.push(END_OF_FILE);
    trailer
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn swizzle_roundtrip() {
        for b in 0u8..=255 {
            assert_eq!(unswizzle_byte(swizzle_byte(b, MAGIC), MAGIC), b);
        }
    }

    #[test]
    fn encode_value_7bit_big_endian() {
        // 0x80 = 128 → zwei 7-Bit-Gruppen: [1, 0].
        assert_eq!(encode_value(128, 2), vec![1, 0]);
        assert_eq!(encode_value(0x7F, 1), vec![0x7F]);
    }

    #[test]
    fn encode_coord_hat_fuenf_bytes() {
        assert_eq!(encode_coord(0).len(), 5);
        assert_eq!(encode_coord(-10_000).len(), 5);
        // 1 mm = 1000 µm → letzte Gruppe trägt den Wert.
        let e = encode_coord(1000);
        assert_eq!(e.len(), 5);
    }

    #[test]
    fn encode_power_volle_leistung() {
        // 100 % → 0x3FFF, verteilt auf 2×7 Bit = [0x7F, 0x7F].
        assert_eq!(encode_power(100.0), vec![0x7F, 0x7F]);
        assert_eq!(encode_power(0.0), vec![0, 0]);
    }

    #[test]
    fn packet_hat_checksum_prefix() {
        let payload = vec![0x88, 0x00, 0x00, 0x00, 0x00, 0x00];
        let pkt = build_packet(&payload, MAGIC);
        assert_eq!(pkt.len(), payload.len() + 2);
        // Checksum = Summe der geswizzelten Bytes.
        let scrambled = swizzle(&payload, MAGIC);
        assert_eq!(&pkt[..2], &checksum(&scrambled));
        assert_eq!(&pkt[2..], &scrambled[..]);
    }

    #[test]
    fn pause_nutzt_referenzkommando() {
        assert_eq!(cmd_pause(), vec![0xD8, 0x02]);
    }
}
