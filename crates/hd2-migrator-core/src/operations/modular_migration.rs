//! Modular armor mod migration engine.
//! Supports component-isolated clothing toggles, split material packages,
//! full topology slot expansion, and anti-pollution detection.

use crate::archive::StreamToc;
use crate::constants::{LEGACY_MAGIC, UNIT_ID};
use crate::unit::authority::ArmorMappingTable;
use crate::unit::helmet_authority::HelmetMappingTable;
use byteorder::{ByteOrder, LittleEndian as LE};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModularComponentInfo {
    pub id: String,
    pub name: String,
    pub patch_files: Vec<String>,
    pub unit_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModularModInspection {
    pub mod_name: String,
    pub author: Option<String>,
    pub version: Option<String>,
    pub detected_source_armor: Option<String>,
    pub detected_source_helmet: Option<String>,
    pub components: Vec<ModularComponentInfo>,
    pub has_manifest: bool,
    pub has_material_pack: bool,
    pub material_patch_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModularSlotMapping {
    pub slot_key: String,
    pub source_unit_id: u64,
    pub source_unit_hex: String,
    pub target_unit_id: u64,
    pub target_unit_hex: String,
    pub is_polluted: bool,
    pub polluted_by: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModularMigrateOptions {
    pub target_armor_name: String,
    pub target_helmet_name: Option<String>,
    pub selected_components: Vec<String>,
    pub strategy: String, // "full_topology" | "smart_merge"
    pub custom_unit_mappings: HashMap<String, String>, // source hex -> target hex
    pub anti_pollution: bool,
    pub output_mod_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ModularMigrationSummary {
    pub output_path: String,
    pub total_files_copied: usize,
    pub total_substitutions: usize,
    pub components_migrated: Vec<String>,
    pub warnings: Vec<String>,
}

/// Known 23-slot table between TG-8 Sharpshooter and CW-9 White Wolf
pub fn get_known_tg_to_cw_mappings() -> Vec<(&'static str, u64, u64)> {
    vec![
        // Core Body Slots (10)
        ("stocky body", 0xD65CF7C1FCBE0991, 0x5EF624A8D1687C29),
        ("slim body", 0x03F604F060D6F93B, 0x83F683152CCAB006),
        ("stocky waist", 0xE6B1A265ACDC64F4, 0x9FA3044EB6CAE4AF),
        ("slim waist", 0x4A2023061DC5FEB2, 0x731CF4EC8C1F9420),
        ("stocky left arm", 0xCF1F1897FEECE209, 0x76EE777C6E314771),
        ("slim left arm", 0xE3992168DCDF2AE7, 0xF22E88824CD6F65A),
        ("stocky right arm", 0x12C175815856E272, 0x1BF2B0AE9EF586EC),
        ("slim right arm", 0xD366C96997B3C1CC, 0x0C42B65D73640DA0),
        ("left leg (socks)", 0x441AAA4D6094867E, 0x2CA550B03BDAE48E),
        ("right leg (shoes)", 0xFB62F74D97A72C4A, 0x64CF3C347EA64880),
        // Clothing Toggle Slots (3)
        ("clothing_1", 0xC86087DDDF60D9DE, 0x8BE41D5A14DC6C3D),
        ("clothing_2", 0xB6C2AEBB55D54FF7, 0xD9A15B40DBACDC43),
        ("underwear", 0x3A1D79EB4636031F, 0x3D2286B17A6F8CCB),
        // Helmet / Head Slot
        ("helmet_head", 0xBFDC0F01475D16C8, 0x6D5011A2D2D35EB8),
        // Accessory Slots (9)
        ("accessory_1", 0x001A106B352493D5, 0x1FD9455B846D683B),
        ("accessory_2", 0x0A09425DA11544D7, 0x471F509CAA49ECE7),
        ("accessory_3", 0x1FC7B7C36F0B1674, 0x5D031FC5A112C740),
        ("accessory_4", 0x2B7598966411556F, 0x874D8531268B0FE4),
        ("accessory_5", 0x2D3304F71768092B, 0xB89536C8E43CEA1D),
        ("accessory_6", 0x7604826C8C96A881, 0xC03B0336C87667BF),
        ("accessory_7", 0xA111DE5354035DD7, 0xD80CACD8A799667F),
        ("accessory_8", 0xB65C90398117EB1F, 0xDE1C1B15BED946D2),
        ("accessory_9", 0xE0EEAE016DA8C9ED, 0x1E3B1681C6540425),
    ]
}

/// Extract all unit file IDs from a raw TOC buffer
pub fn extract_unit_ids_from_toc_buffer(buf: &[u8]) -> Vec<u64> {
    if buf.len() < 72 {
        return Vec::new();
    }
    let magic = LE::read_u32(&buf[0..4]);
    if magic != LEGACY_MAGIC {
        return Vec::new();
    }
    let num_types = LE::read_u32(&buf[4..8]) as usize;
    let num_files = LE::read_u32(&buf[8..12]) as usize;
    let types_start = 72;
    let entries_start = types_start + num_types * 32;
    let entries_end = entries_start + num_files * 80;
    if buf.len() < entries_end {
        return Vec::new();
    }

    let mut units = Vec::new();
    for i in 0..num_files {
        let entry_offset = entries_start + i * 80;
        let file_id = LE::read_u64(&buf[entry_offset..entry_offset + 8]);
        let type_id = LE::read_u64(&buf[entry_offset + 8..entry_offset + 16]);
        if type_id == UNIT_ID {
            units.push(file_id);
        }
    }
    units
}

/// Build unit usage index to detect armor pollution
pub fn build_unit_pollution_index(table: &ArmorMappingTable) -> HashMap<u64, Vec<String>> {
    let mut index = HashMap::<u64, Vec<String>>::new();
    for (name, parts) in table.entries() {
        for &id in parts.parts().values() {
            index.entry(id).or_default().push(name.to_string());
        }
    }
    index
}

/// Inspect detected source armor from unit set
pub fn detect_source_armor(
    units: &HashSet<u64>,
    table: &ArmorMappingTable,
) -> Option<(String, usize)> {
    let mut best_armor: Option<(String, usize)> = None;
    for (name, parts) in table.entries() {
        let mut matches = 0;
        for &id in parts.parts().values() {
            if units.contains(&id) {
                matches += 1;
            }
        }
        if matches > 0 {
            if let Some((_, best_count)) = &best_armor {
                if matches > *best_count {
                    best_armor = Some((name.to_string(), matches));
                }
            } else {
                best_armor = Some((name.to_string(), matches));
            }
        }
    }
    best_armor
}

/// Build mapping slots between source armor and target armor
pub fn build_slot_mappings(
    source_armor: &str,
    target_armor: &str,
    source_helmet: Option<&str>,
    target_helmet: Option<&str>,
    custom_overrides: &HashMap<String, String>,
    armor_table: &ArmorMappingTable,
    helmet_table: &HelmetMappingTable,
    pollution_index: &HashMap<u64, Vec<String>>,
) -> Vec<ModularSlotMapping> {
    let mut mappings = Vec::new();
    let mut mapped_source_ids = HashSet::new();

    // Check if source and target match known TG-8 -> CW-9
    let is_tg_to_cw = (source_armor.contains("TG-8") || source_armor.contains("Sharpshooter"))
        && (target_armor.contains("CW-9") || target_armor.contains("White Wolf"));

    if is_tg_to_cw {
        for (slot, src_id, dst_id) in get_known_tg_to_cw_mappings() {
            let src_hex = format!("0x{:016X}", src_id);
            let dst_id_final = if let Some(custom_dst_hex) = custom_overrides.get(&src_hex) {
                u64::from_str_radix(custom_dst_hex.trim_start_matches("0x"), 16).unwrap_or(dst_id)
            } else {
                dst_id
            };
            let dst_hex = format!("0x{:016X}", dst_id_final);

            let mut polluted_by = Vec::new();
            if let Some(armors) = pollution_index.get(&dst_id_final) {
                for a in armors {
                    if a != target_armor && !polluted_by.contains(a) {
                        polluted_by.push(a.clone());
                    }
                }
            }
            let is_polluted = !polluted_by.is_empty();

            mappings.push(ModularSlotMapping {
                slot_key: slot.to_string(),
                source_unit_id: src_id,
                source_unit_hex: src_hex,
                target_unit_id: dst_id_final,
                target_unit_hex: dst_hex,
                is_polluted,
                polluted_by,
            });
            mapped_source_ids.insert(src_id);
        }
        return mappings;
    }

    // General Case: Core 10 Parts from ArmorMappingTable
    if let (Some(src_parts), Some(dst_parts)) = (armor_table.armor(source_armor), armor_table.armor(target_armor)) {
        for (part_label, &src_id) in src_parts.parts() {
            if let Some(&dst_id) = dst_parts.get(part_label) {
                let src_hex = format!("0x{:016X}", src_id);
                let dst_id_final = if let Some(custom_dst_hex) = custom_overrides.get(&src_hex) {
                    u64::from_str_radix(custom_dst_hex.trim_start_matches("0x"), 16).unwrap_or(dst_id)
                } else {
                    dst_id
                };
                let dst_hex = format!("0x{:016X}", dst_id_final);

                let mut polluted_by = Vec::new();
                if let Some(armors) = pollution_index.get(&dst_id_final) {
                    for a in armors {
                        if a != target_armor && !polluted_by.contains(a) {
                            polluted_by.push(a.clone());
                        }
                    }
                }
                let is_polluted = !polluted_by.is_empty();

                mappings.push(ModularSlotMapping {
                    slot_key: part_label.clone(),
                    source_unit_id: src_id,
                    source_unit_hex: src_hex,
                    target_unit_id: dst_id_final,
                    target_unit_hex: dst_hex,
                    is_polluted,
                    polluted_by,
                });
                mapped_source_ids.insert(src_id);
            }
        }
    }

    // Helmet mapping
    let src_helmet_id = source_helmet.and_then(|h| helmet_table.unit_id(h));
    let dst_helmet_id = target_helmet.and_then(|h| helmet_table.unit_id(h))
        .or_else(|| helmet_table.unit_id(target_armor)); // Many helmets share exact armor base name

    if let (Some(src_hid), Some(dst_hid)) = (src_helmet_id, dst_helmet_id) {
        if !mapped_source_ids.contains(&src_hid) {
            let src_hex = format!("0x{:016X}", src_hid);
            let dst_id_final = if let Some(custom_dst_hex) = custom_overrides.get(&src_hex) {
                u64::from_str_radix(custom_dst_hex.trim_start_matches("0x"), 16).unwrap_or(dst_hid)
            } else {
                dst_hid
            };
            let dst_hex = format!("0x{:016X}", dst_id_final);

            mappings.push(ModularSlotMapping {
                slot_key: "Helmet".to_string(),
                source_unit_id: src_hid,
                source_unit_hex: src_hex,
                target_unit_id: dst_id_final,
                target_unit_hex: dst_hex,
                is_polluted: false,
                polluted_by: Vec::new(),
            });
            mapped_source_ids.insert(src_hid);
        }
    }

    // Custom overrides for any additional accessory slots
    for (src_hex, dst_hex) in custom_overrides {
        let src_id = u64::from_str_radix(src_hex.trim_start_matches("0x"), 16).unwrap_or(0);
        let dst_id = u64::from_str_radix(dst_hex.trim_start_matches("0x"), 16).unwrap_or(0);
        if src_id != 0 && dst_id != 0 && !mapped_source_ids.contains(&src_id) {
            mappings.push(ModularSlotMapping {
                slot_key: format!("Custom ({src_hex})"),
                source_unit_id: src_id,
                source_unit_hex: src_hex.clone(),
                target_unit_id: dst_id,
                target_unit_hex: dst_hex.clone(),
                is_polluted: false,
                polluted_by: Vec::new(),
            });
        }
    }

    mappings
}

/// Replace all 8-byte aligned occurrences of source unit IDs with target unit IDs in a byte slice
pub fn replace_unit_hashes(buf: &mut [u8], replacements: &[(u64, u64)]) -> usize {
    if buf.len() < 8 || replacements.is_empty() {
        return 0;
    }
    let mut total_replacements = 0;
    let limit = buf.len() - 8;
    let mut i = 0;
    while i <= limit {
        let val = LE::read_u64(&buf[i..i + 8]);
        let mut matched = false;
        for &(src, dst) in replacements {
            if val == src {
                LE::write_u64(&mut buf[i..i + 8], dst);
                total_replacements += 1;
                matched = true;
                break;
            }
        }
        if matched {
            i += 8;
        } else {
            i += 1;
        }
    }
    total_replacements
}

/// Simple random UUID v4 generator for manifest GUIDs
pub fn generate_uuid_v4() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    let mut bytes = [0u8; 16];
    let n1 = (now & 0xFFFFFFFFFFFFFFFF) as u64;
    let n2 = ((now >> 64) & 0xFFFFFFFFFFFFFFFF) as u64 ^ 0x9E3779B97F4A7C15;
    LE::write_u64(&mut bytes[0..8], n1);
    LE::write_u64(&mut bytes[8..16], n2);
    // UUID v4 format
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

/// Update or generate an Arsenal Mod Manager manifest.json string (UTF-8 without BOM)
pub fn update_manifest_json(
    original_json: Option<&str>,
    new_mod_name: &str,
    selected_components: &[String],
) -> String {
    if let Some(raw) = original_json {
        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(raw) {
            if let Some(obj) = val.as_object_mut() {
                obj.insert("Name".to_string(), serde_json::Value::String(new_mod_name.to_string()));
                obj.insert("Guid".to_string(), serde_json::Value::String(generate_uuid_v4()));
                // If components are selectively filtered
                if !selected_components.is_empty() {
                    if let Some(opts) = obj.get_mut("Options").and_then(|o| o.as_array_mut()) {
                        opts.retain(|opt| {
                            if let Some(name) = opt.get("Name").and_then(|n| n.as_str()) {
                                selected_components.iter().any(|c| c == name)
                            } else {
                                true
                            }
                        });
                    }
                }
                if let Ok(serialized) = serde_json::to_string_pretty(&val) {
                    return serialized;
                }
            }
        }
    }

    // Default clean manifest
    let guid = generate_uuid_v4();
    let options: Vec<serde_json::Value> = selected_components
        .iter()
        .map(|comp| {
            serde_json::json!({
                "Name": comp,
                "Description": comp,
                "Enabled": true,
                "Files": [format!("{}/", comp)]
            })
        })
        .collect();

    let manifest = serde_json::json!({
        "Name": new_mod_name,
        "Author": "Modular Migrator",
        "Version": "1.0.0",
        "Guid": guid,
        "Options": options
    });

    serde_json::to_string_pretty(&manifest).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uuid_v4_format() {
        let uuid = generate_uuid_v4();
        assert_eq!(uuid.len(), 36);
        assert_eq!(&uuid[14..15], "4");
        assert!(uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[test]
    fn test_replace_unit_hashes() {
        let mut buf = vec![0u8; 32];
        let src: u64 = 0x0123456789ABCDEF;
        let dst: u64 = 0xFEDCBA9876543210;
        LE::write_u64(&mut buf[8..16], src);
        LE::write_u64(&mut buf[16..24], src);

        let replaced = replace_unit_hashes(&mut buf, &[(src, dst)]);
        assert_eq!(replaced, 2);
        assert_eq!(LE::read_u64(&buf[8..16]), dst);
        assert_eq!(LE::read_u64(&buf[16..24]), dst);
    }
}
