use super::output::{create_zip, finish_zip, write_zip_entry_with_progress, OutputProgress};
use super::DesktopProgress;
use crate::command_error::CommandError;
use hd2_migrator_core::operations::modular_migration::{
    build_slot_mappings, build_unit_pollution_index, detect_source_armor,
    extract_unit_ids_from_toc_buffer, replace_unit_hashes, update_manifest_json,
    ModularComponentInfo, ModularMigrateOptions, ModularMigrationSummary,
    ModularModInspection, ModularSlotMapping,
};
use hd2_migrator_core::unit::authority::ArmorMappingTable;
use hd2_migrator_core::unit::helmet_authority::HelmetMappingTable;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectModularModRequest {
    pub mod_path: PathBuf,
    pub material_pack_path: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewModularMappingsRequest {
    pub source_armor: String,
    pub target_armor: String,
    pub source_helmet: Option<String>,
    pub target_helmet: Option<String>,
    pub custom_overrides: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModularMigrateRequest {
    pub mod_path: PathBuf,
    pub material_pack_path: Option<PathBuf>,
    pub output_path: PathBuf,
    pub options: ModularMigrateOptions,
}

pub fn inspect_modular_mod_blocking(
    request: InspectModularModRequest,
) -> Result<ModularModInspection, String> {
    let files = read_source_files(&request.mod_path)?;
    let mut manifest_content: Option<String> = None;
    let mut component_files: HashMap<String, Vec<String>> = HashMap::new();
    let mut component_units: HashMap<String, HashSet<u64>> = HashMap::new();
    let mut all_units: HashSet<u64> = HashSet::new();

    // Scan files in mod
    for (rel_path, bytes) in &files {
        let norm_path = rel_path.replace('\\', "/");
        if norm_path.eq_ignore_ascii_case("manifest.json") {
            manifest_content = String::from_utf8(bytes.clone()).ok();
            continue;
        }

        // Determine component name: first path segment if in folder, or file stem if root
        let comp_name = if let Some(idx) = norm_path.find('/') {
            norm_path[..idx].to_string()
        } else if norm_path.contains(".patch_") {
            "Root".to_string()
        } else {
            "Extra".to_string()
        };

        component_files.entry(comp_name.clone()).or_default().push(norm_path.clone());

        if is_patch_file(&norm_path) {
            let units = extract_unit_ids_from_toc_buffer(bytes);
            for u in units {
                all_units.insert(u);
                component_units.entry(comp_name.clone()).or_default().insert(u);
            }
        }
    }

    // Material pack inspection if provided
    let mut material_patch_files = Vec::new();
    let has_material_pack = if let Some(mat_path) = request.material_pack_path.as_ref() {
        if let Ok(mat_files) = read_source_files(mat_path) {
            for (p, b) in &mat_files {
                let norm = p.replace('\\', "/");
                if is_patch_file(&norm) {
                    material_patch_files.push(norm);
                    for u in extract_unit_ids_from_toc_buffer(b) {
                        all_units.insert(u);
                    }
                }
            }
            !mat_files.is_empty()
        } else {
            false
        }
    } else {
        false
    };

    // Detect Source Armor & Helmet
    let armor_table = ArmorMappingTable::bundled().map_err(|e| e.to_string())?;
    let helmet_table = HelmetMappingTable::bundled().map_err(|e| e.to_string())?;

    let detected_source_armor = detect_source_armor(&all_units, &armor_table).map(|(name, _)| name);

    let mut detected_source_helmet: Option<String> = None;
    for (name, unit_id) in helmet_table.entries() {
        if all_units.contains(&unit_id) {
            detected_source_helmet = Some(name.to_string());
            break;
        }
    }
    // Specific TG-8 Helmet fallback check
    if detected_source_helmet.is_none() && all_units.contains(&0xBFDC0F01475D16C8) {
        detected_source_helmet = Some("TG-8 Sharpshooter".to_string());
    }

    // Parse manifest if available
    let (mod_name, author, version) = if let Some(raw) = manifest_content.as_deref() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(raw) {
            (
                json.get("Name").and_then(|v| v.as_str()).map(ToString::to_string),
                json.get("Author").and_then(|v| v.as_str()).map(ToString::to_string),
                json.get("Version").and_then(|v| v.as_str()).map(ToString::to_string),
            )
        } else {
            (None, None, None)
        }
    } else {
        (None, None, None)
    };

    let default_name = request
        .mod_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("HD2 Modular Mod")
        .to_string();

    let mut components = Vec::new();
    for (comp_name, p_files) in component_files {
        let u_ids: Vec<String> = component_units
            .get(&comp_name)
            .map(|set| set.iter().map(|id| format!("0x{:016X}", id)).collect())
            .unwrap_or_default();

        components.push(ModularComponentInfo {
            id: comp_name.clone(),
            name: comp_name,
            patch_files: p_files,
            unit_ids: u_ids,
        });
    }

    // Sort components: Put "本体" or "Root" first
    components.sort_by(|a, b| {
        if a.name == "本体" || a.name == "Root" {
            std::cmp::Ordering::Less
        } else if b.name == "本体" || b.name == "Root" {
            std::cmp::Ordering::Greater
        } else {
            a.name.cmp(&b.name)
        }
    });

    Ok(ModularModInspection {
        mod_name: mod_name.unwrap_or(default_name),
        author,
        version,
        detected_source_armor,
        detected_source_helmet,
        components,
        has_manifest: manifest_content.is_some(),
        has_material_pack,
        material_patch_files,
    })
}

pub fn preview_modular_mappings_blocking(
    request: PreviewModularMappingsRequest,
) -> Result<Vec<ModularSlotMapping>, String> {
    let armor_table = ArmorMappingTable::bundled().map_err(|e| e.to_string())?;
    let helmet_table = HelmetMappingTable::bundled().map_err(|e| e.to_string())?;
    let pollution_index = build_unit_pollution_index(&armor_table);

    let mappings = build_slot_mappings(
        &request.source_armor,
        &request.target_armor,
        request.source_helmet.as_deref(),
        request.target_helmet.as_deref(),
        &request.custom_overrides,
        &armor_table,
        &helmet_table,
        &pollution_index,
    );

    Ok(mappings)
}

pub fn migrate_modular_mod_blocking(
    request: ModularMigrateRequest,
    progress: Option<&DesktopProgress>,
) -> Result<ModularMigrationSummary, String> {
    if let Some(p) = progress {
        p.ensure_active().map_err(|e| e.to_string())?;
        let _ = p.emit(
            super::ProgressKind::Stage,
            &request.options.target_armor_name,
            "",
            "reading_files",
        );
    }

    let source_files = read_source_files(&request.mod_path)?;
    let material_files = if let Some(mat_path) = request.material_pack_path.as_ref() {
        read_source_files(mat_path).unwrap_or_default()
    } else {
        HashMap::new()
    };

    let armor_table = ArmorMappingTable::bundled().map_err(|e| e.to_string())?;
    let helmet_table = HelmetMappingTable::bundled().map_err(|e| e.to_string())?;
    let pollution_index = build_unit_pollution_index(&armor_table);

    // Build replacement pairs
    let mappings = build_slot_mappings(
        "TG-8 Sharpshooter", // will use known or general
        &request.options.target_armor_name,
        None,
        request.options.target_helmet_name.as_deref(),
        &request.options.custom_unit_mappings,
        &armor_table,
        &helmet_table,
        &pollution_index,
    );

    let mut replacements: Vec<(u64, u64)> = Vec::new();
    let mut warnings = Vec::new();

    for m in &mappings {
        if m.is_polluted && request.options.anti_pollution {
            warnings.push(format!(
                "Slot '{}' skipped due to anti-pollution guard (Shared with {})",
                m.slot_key,
                m.polluted_by.join(", ")
            ));
            continue;
        }
        if m.is_polluted {
            warnings.push(format!(
                "Notice: Slot '{}' is shared with {}",
                m.slot_key,
                m.polluted_by.join(", ")
            ));
        }
        replacements.push((m.source_unit_id, m.target_unit_id));
    }

    let mut zip = create_zip(&request.output_path)?;
    let output_progress = progress.map(|v| v as &dyn OutputProgress);

    let mut total_substitutions = 0;
    let mut total_files_copied = 0;
    let mut manifest_raw: Option<String> = None;
    let selected_comps: HashSet<String> = request
        .options
        .selected_components
        .iter()
        .cloned()
        .collect();

    if let Some(p) = progress {
        let _ = p.emit(
            super::ProgressKind::Stage,
            &request.options.target_armor_name,
            "",
            "processing_components",
        );
    }

    // Process source mod files
    for (rel_path, mut bytes) in source_files {
        let norm_path = rel_path.replace('\\', "/");
        if norm_path.eq_ignore_ascii_case("manifest.json") {
            manifest_raw = String::from_utf8(bytes).ok();
            continue;
        }

        // Component filtering
        let comp_name = if let Some(idx) = norm_path.find('/') {
            &norm_path[..idx]
        } else {
            "Root"
        };
        if !selected_comps.is_empty() && !selected_comps.contains(comp_name) && comp_name != "Root" {
            continue;
        }

        if is_patch_file(&norm_path) {
            let replaced = replace_unit_hashes(&mut bytes, &replacements);
            total_substitutions += replaced;
        }

        write_zip_entry_with_progress(&mut zip, &norm_path, &bytes, output_progress)
            .map_err(|e| e.to_string())?;
        total_files_copied += 1;
    }

    // Process material pack files if any
    for (rel_path, mut bytes) in material_files {
        let norm_path = rel_path.replace('\\', "/");
        if is_patch_file(&norm_path) {
            let replaced = replace_unit_hashes(&mut bytes, &replacements);
            total_substitutions += replaced;
        }
        write_zip_entry_with_progress(&mut zip, &norm_path, &bytes, output_progress)
            .map_err(|e| e.to_string())?;
        total_files_copied += 1;
    }

    // Write updated manifest.json
    let new_name = request.options.output_mod_name.unwrap_or_else(|| {
        format!("Modular Mod 替换 {}", request.options.target_armor_name)
    });
    let final_manifest_str = update_manifest_json(
        manifest_raw.as_deref(),
        &new_name,
        &request.options.selected_components,
    );
    write_zip_entry_with_progress(
        &mut zip,
        "manifest.json",
        final_manifest_str.as_bytes(),
        output_progress,
    )
    .map_err(|e| e.to_string())?;
    total_files_copied += 1;

    finish_zip(zip)?;

    if let Some(p) = progress {
        let _ = p.emit(
            super::ProgressKind::TargetFinish,
            &request.options.target_armor_name,
            "",
            "completed",
        );
    }

    Ok(ModularMigrationSummary {
        output_path: request.output_path.to_string_lossy().to_string(),
        total_files_copied,
        total_substitutions,
        components_migrated: request.options.selected_components,
        warnings,
    })
}

fn is_patch_file(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.contains(".patch_")
        && !lower.ends_with(".gpu_resources")
        && !lower.ends_with(".stream")
        && !lower.ends_with(".png")
}

fn read_source_files(path: &Path) -> Result<HashMap<String, Vec<u8>>, String> {
    let mut map = HashMap::new();
    if path.is_file() {
        // ZIP archive
        let file = File::open(path).map_err(|e| format!("Open ZIP {}: {e}", path.display()))?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| format!("Read ZIP archive {}: {e}", path.display()))?;
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| format!("Read ZIP entry {i}: {e}"))?;
            if entry.is_file() {
                let name = entry.name().to_string();
                let mut data = Vec::with_capacity(entry.size() as usize);
                entry
                    .read_to_end(&mut data)
                    .map_err(|e| format!("Extract ZIP entry {name}: {e}"))?;
                map.insert(name, data);
            }
        }
    } else if path.is_dir() {
        // Directory
        for entry in walkdir::WalkDir::new(path) {
            let entry = entry.map_err(|e| format!("Walkdir error: {e}"))?;
            if entry.file_type().is_file() {
                let rel = entry
                    .path()
                    .strip_prefix(path)
                    .map_err(|e| format!("Strip prefix {}: {e}", entry.path().display()))?;
                let rel_str = rel.to_string_lossy().replace('\\', "/");
                let data = std::fs::read(entry.path())
                    .map_err(|e| format!("Read file {}: {e}", entry.path().display()))?;
                map.insert(rel_str, data);
            }
        }
    } else {
        return Err(format!("Source path does not exist: {}", path.display()));
    }
    Ok(map)
}
