//! Design-first-Layerkonfiguration (ADR 0019, experimentell/uncommitted).

use crate::ui::{LayerManagerState, MaterialManagerState};

use super::App;

impl App {
    pub fn open_layer_manager(&mut self) {
        if self.session.layers.is_empty() {
            self.toasts.error("Das Design enthält noch keine Layer.");
            return;
        }
        let material_id = self
            .laser_backend
            .active_profile()
            .and_then(|laser| self.material_service.library().active_for(&laser.id))
            .map(|material| material.id.clone());
        self.layer_manager = Some(LayerManagerState {
            layers: self
                .session
                .layers
                .iter()
                .map(luxifer_application::LayerParams::from_layer)
                .collect(),
            material_id,
        });
    }

    pub fn layer_manager_load_material(&mut self) {
        let Some(manager) = self.layer_manager.as_mut() else {
            return;
        };
        let Some(material_id) = manager.material_id.as_deref() else {
            return;
        };
        if let Err(error) = self
            .material_service
            .apply_profile(material_id, &mut manager.layers)
        {
            self.app_error = Some(error);
        }
    }

    pub fn layer_manager_save(&mut self) {
        let Some(draft) = self.layer_manager.as_ref() else {
            return;
        };
        match self.session.set_all_layer_params(&draft.layers) {
            Ok(()) => {
                if let Some(laser) = self.laser_backend.active_profile() {
                    if let Err(error) = self
                        .material_service
                        .set_active(&laser.id, draft.material_id.as_deref())
                    {
                        self.app_error = Some(error);
                        return;
                    }
                }
                self.layer_manager = None;
                self.renderer.invalidate_scene();
                self.toasts.success("Layerkonfiguration übernommen.");
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn open_material_manager(&mut self, create_new: bool) {
        let Some(laser_id) = self
            .laser_backend
            .active_profile()
            .map(|profile| profile.id.clone())
        else {
            self.toasts.error("Zuerst einen Laser auswählen.");
            return;
        };

        let source = self
            .layer_manager
            .as_ref()
            .and_then(|manager| manager.layers.first())
            .cloned()
            .unwrap_or_else(|| {
                luxifer_application::LayerParams::from_layer(&luxifer_core::Layer::new(0))
            });
        let selected_id = self
            .layer_manager
            .as_ref()
            .and_then(|manager| manager.material_id.as_deref());
        let selected = selected_id.and_then(|id| {
            self.material_service
                .library()
                .profiles
                .iter()
                .find(|profile| profile.id == id && profile.laser_id == laser_id)
                .cloned()
        });
        let draft = if create_new {
            self.material_service.new_profile(&laser_id, &source)
        } else {
            selected.unwrap_or_else(|| self.material_service.new_profile(&laser_id, &source))
        };
        self.material_manager = Some(MaterialManagerState {
            is_new: create_new || draft.id.is_empty(),
            draft,
        });
    }

    pub fn material_manager_save(&mut self) {
        let Some(profile) = self
            .material_manager
            .as_ref()
            .map(|state| state.draft.clone())
        else {
            return;
        };
        match self.material_service.save_profile(profile) {
            Ok(profile) => {
                if let Some(manager) = self.layer_manager.as_mut() {
                    manager.material_id = Some(profile.id);
                }
                self.material_manager = None;
                self.toasts.success("Materialprofil gespeichert.");
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn material_manager_delete(&mut self) {
        let Some(profile) = self
            .material_manager
            .as_ref()
            .map(|state| state.draft.clone())
        else {
            return;
        };
        match self.material_service.delete_profile(&profile.id) {
            Ok(()) => {
                if let Some(manager) = self.layer_manager.as_mut() {
                    if manager.material_id.as_deref() == Some(profile.id.as_str()) {
                        manager.material_id = None;
                    }
                }
                self.material_manager = None;
                self.toasts.success("Materialprofil gelöscht.");
            }
            Err(error) => self.app_error = Some(error),
        }
    }
}
