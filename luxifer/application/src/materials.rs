//! UI-unabhängiger Materialprofil-Anwendungsfall: validieren, anwenden und
//! atomar persistieren. Das Core besitzt nur das serialisierbare Fachmodell.

use std::path::{Path, PathBuf};

use luxifer_core::{MaterialLibrary, MaterialProcess, MaterialProcessDefaults, MaterialProfile};

use crate::{AppError, LayerParams};

const MATERIAL_FILE: &str = "material-profile.json";

#[derive(Debug)]
pub struct MaterialService {
    library: MaterialLibrary,
    path: PathBuf,
}

impl MaterialService {
    pub fn load() -> Result<Self, AppError> {
        Self::load_from(&luxifer_core::data_root())
    }

    pub fn load_from(dir: &Path) -> Result<Self, AppError> {
        let path = dir.join(MATERIAL_FILE);
        let library = match std::fs::read_to_string(&path) {
            Ok(json) => serde_json::from_str(&json).map_err(|error| {
                AppError::wrap(
                    "material_library_read",
                    "Materialbibliothek ist beschädigt und wurde nicht überschrieben.",
                    error.to_string(),
                )
            })?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                MaterialLibrary::default()
            }
            Err(error) => {
                return Err(AppError::wrap(
                    "material_library_read",
                    "Materialbibliothek konnte nicht gelesen werden.",
                    error.to_string(),
                ));
            }
        };
        Ok(Self { library, path })
    }

    pub fn library(&self) -> &MaterialLibrary {
        &self.library
    }

    /// Ersetzt die lokale Bibliothek durch eine geprüfte Arbeitsplatzsicherung.
    /// Persistenz und laufender Zustand bleiben auch hier transaktional gekoppelt.
    pub fn restore_library(&mut self, library: MaterialLibrary) -> Result<(), AppError> {
        for profile in &library.profiles {
            profile
                .validate()
                .map_err(|message| AppError::new("invalid_material_profile", message))?;
        }
        for (laser_id, material_id) in &library.active_by_laser {
            if !library
                .profiles
                .iter()
                .any(|profile| profile.id == *material_id && profile.laser_id == *laser_id)
            {
                return Err(AppError::new(
                    "invalid_material_selection",
                    "Die gesicherte Materialauswahl passt nicht zum Laser.",
                ));
            }
        }
        self.commit(library)
    }

    pub fn new_profile(&self, laser_id: &str, layer: &LayerParams) -> MaterialProfile {
        let process = MaterialProcess::from_layer_mode(layer.mode);
        let mut profile = MaterialProfile {
            id: String::new(),
            laser_id: laser_id.into(),
            name: "Neues Material".into(),
            thickness_mm: None,
            cut: None,
            vector_engrave: None,
            raster_engrave: None,
        };
        *profile.defaults_mut(process) = Some(defaults_from_params(layer));
        profile
    }

    pub fn save_profile(
        &mut self,
        mut profile: MaterialProfile,
    ) -> Result<MaterialProfile, AppError> {
        profile.name = profile.name.trim().to_string();
        profile
            .validate()
            .map_err(|message| AppError::new("invalid_material_profile", message))?;
        if profile.id.is_empty() {
            profile.id = new_material_id();
        }
        let mut next = self.library.clone();
        if let Some(existing) = next.profiles.iter_mut().find(|item| item.id == profile.id) {
            *existing = profile.clone();
        } else {
            next.profiles.push(profile.clone());
        }
        if !next.set_active(&profile.laser_id, Some(&profile.id)) {
            return Err(AppError::new(
                "invalid_material_selection",
                "Material konnte dem Laser nicht zugeordnet werden.",
            ));
        }
        self.commit(next)?;
        Ok(profile)
    }

    pub fn delete_profile(&mut self, id: &str) -> Result<(), AppError> {
        if !self.library.profiles.iter().any(|profile| profile.id == id) {
            return Err(AppError::new(
                "material_not_found",
                "Das Materialprofil existiert nicht mehr.",
            ));
        }
        let mut next = self.library.clone();
        next.profiles.retain(|profile| profile.id != id);
        next.active_by_laser
            .retain(|_, material_id| material_id != id);
        self.commit(next)
    }

    pub fn set_active(
        &mut self,
        laser_id: &str,
        material_id: Option<&str>,
    ) -> Result<(), AppError> {
        let mut next = self.library.clone();
        if !next.set_active(laser_id, material_id) {
            return Err(AppError::new(
                "invalid_material_selection",
                "Das Material gehört nicht zum aktiven Laser.",
            ));
        }
        self.commit(next)
    }

    pub fn apply_profile(
        &self,
        profile_id: &str,
        layers: &mut [LayerParams],
    ) -> Result<(), AppError> {
        let profile = self
            .library
            .profiles
            .iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| {
                AppError::new(
                    "material_not_found",
                    "Das Materialprofil existiert nicht mehr.",
                )
            })?;
        for layer in layers {
            let process = MaterialProcess::from_layer_mode(layer.mode);
            if let Some(defaults) = profile.defaults(process) {
                apply_defaults(defaults, layer);
            }
        }
        Ok(())
    }

    fn commit(&mut self, next: MaterialLibrary) -> Result<(), AppError> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| AppError::new("material_library_write", "Ungültiger Speicherort."))?;
        std::fs::create_dir_all(parent).map_err(material_write_error)?;
        let bytes = serde_json::to_vec_pretty(&next).map_err(|error| {
            AppError::wrap(
                "material_library_write",
                "Materialbibliothek konnte nicht serialisiert werden.",
                error.to_string(),
            )
        })?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, bytes).map_err(material_write_error)?;
        std::fs::rename(&temp, &self.path).map_err(material_write_error)?;
        self.library = next;
        Ok(())
    }
}

fn defaults_from_params(layer: &LayerParams) -> MaterialProcessDefaults {
    MaterialProcessDefaults {
        speed_mm_s: layer.speed_mm_s,
        power_pct: layer.power_pct,
        min_power_pct: layer.min_power_pct,
        passes: layer.passes,
        air_assist: layer.air_assist,
        line_step_mm: layer.line_step_mm,
        dpi: layer.dpi,
        bidirectional: layer.bidirectional,
    }
}

fn apply_defaults(defaults: &MaterialProcessDefaults, layer: &mut LayerParams) {
    layer.speed_mm_s = defaults.speed_mm_s;
    layer.power_pct = defaults.power_pct;
    layer.min_power_pct = defaults.min_power_pct;
    layer.passes = defaults.passes;
    layer.air_assist = defaults.air_assist;
    layer.line_step_mm = defaults.line_step_mm;
    layer.dpi = defaults.dpi;
    layer.bidirectional = defaults.bidirectional;
}

fn new_material_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("material-{nanos}")
}

fn material_write_error(error: std::io::Error) -> AppError {
    AppError::wrap(
        "material_library_write",
        "Materialbibliothek konnte nicht gespeichert werden.",
        error.to_string(),
    )
}

#[cfg(test)]
fn temp_dir(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "luxifer_material_{tag}_{}_{}",
        std::process::id(),
        new_material_id()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layer(mode: luxifer_core::LayerMode) -> LayerParams {
        let mut layer = LayerParams::from_layer(&luxifer_core::Layer::new(0));
        layer.mode = mode;
        layer
    }

    #[test]
    fn speichern_laden_und_keine_auswahl_sind_persistent() {
        let dir = temp_dir("service");
        let mut service = MaterialService::load_from(&dir).unwrap();
        let profile = service.new_profile("laser-a", &layer(luxifer_core::LayerMode::Cut));
        let saved = service.save_profile(profile).unwrap();
        service.set_active("laser-a", None).unwrap();

        let loaded = MaterialService::load_from(&dir).unwrap();
        assert_eq!(loaded.library().profiles[0].id, saved.id);
        assert!(loaded.library().active_for("laser-a").is_none());
    }

    #[test]
    fn materialwerte_werden_prozessbezogen_auf_layerentwurf_angewandt() {
        let dir = temp_dir("apply");
        let mut service = MaterialService::load_from(&dir).unwrap();
        let mut profile = service.new_profile("laser-a", &layer(luxifer_core::LayerMode::Cut));
        profile.cut.as_mut().unwrap().speed_mm_s = 12.0;
        profile.vector_engrave = Some(MaterialProcessDefaults {
            speed_mm_s: 220.0,
            ..profile.cut.clone().unwrap()
        });
        let profile = service.save_profile(profile).unwrap();
        let mut layers = vec![
            layer(luxifer_core::LayerMode::Cut),
            layer(luxifer_core::LayerMode::Fill),
        ];
        service.apply_profile(&profile.id, &mut layers).unwrap();
        assert_eq!(layers[0].speed_mm_s, 12.0);
        assert_eq!(layers[1].speed_mm_s, 220.0);
    }

    #[test]
    fn beschaedigte_bibliothek_wird_nicht_still_geleert() {
        let dir = temp_dir("corrupt");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(MATERIAL_FILE), "{kaputt").unwrap();
        assert_eq!(
            MaterialService::load_from(&dir).unwrap_err().code(),
            "material_library_read"
        );
    }

    #[test]
    fn fehlgeschlagenes_speichern_mutiert_den_laufenden_zustand_nicht() {
        let dir = temp_dir("write-failure");
        std::fs::create_dir_all(&dir).unwrap();
        let blocked_parent = dir.join("keine-map");
        std::fs::write(&blocked_parent, "ist eine Datei").unwrap();
        let mut service = MaterialService {
            library: MaterialLibrary::default(),
            path: blocked_parent.join(MATERIAL_FILE),
        };
        let profile = service.new_profile("laser-a", &layer(luxifer_core::LayerMode::Cut));

        assert!(service.save_profile(profile).is_err());
        assert!(service.library().profiles.is_empty());
        assert!(service.library().active_by_laser.is_empty());
    }

    #[test]
    fn wiederherstellung_validiert_aktive_laserzuordnung() {
        let dir = temp_dir("restore-invalid");
        let mut service = MaterialService::load_from(&dir).unwrap();
        let mut library = MaterialLibrary::default();
        library
            .active_by_laser
            .insert("laser-a".into(), "fehlt".into());

        assert!(service.restore_library(library).is_err());
        assert!(service.library().active_by_laser.is_empty());
    }
}
