//! Lebenszyklus der eigenständigen Laser-Master-Detail-Verwaltung.

use crate::ui::{LaserManagerState, LaserManagerTab};

use super::App;

impl App {
    pub fn open_laser_manager(&mut self, create_new: bool) {
        let profile = if create_new {
            luxifer_core::LaserProfile::default()
        } else {
            self.laser_backend
                .active_profile()
                .cloned()
                .unwrap_or_default()
        };
        let selected_id = (!profile.id.is_empty()).then(|| profile.id.clone());
        self.laser_manager = Some(LaserManagerState {
            selected_id,
            draft: profile,
            is_new: create_new,
            tab: LaserManagerTab::Grunddaten,
            machine_settings: Vec::new(),
            machine_dirty: Default::default(),
            machine_confirm_write: false,
        });
    }

    pub fn laser_manager_select(&mut self, id: &str) {
        let Some(profile) = self
            .laser_backend
            .registry
            .profiles
            .iter()
            .find(|p| p.id == id)
            .cloned()
        else {
            return;
        };
        if let Some(st) = self.laser_manager.as_mut() {
            st.selected_id = Some(id.into());
            st.draft = profile;
            st.is_new = false;
            st.tab = LaserManagerTab::Grunddaten;
            st.machine_settings.clear();
            st.machine_dirty.clear();
            st.machine_confirm_write = false;
        }
    }

    pub fn laser_manager_new(&mut self) {
        if let Some(st) = self.laser_manager.as_mut() {
            st.selected_id = None;
            st.draft = Default::default();
            st.is_new = true;
            st.tab = LaserManagerTab::Grunddaten;
            st.machine_settings.clear();
            st.machine_dirty.clear();
            st.machine_confirm_write = false;
        }
    }

    pub fn laser_manager_save(&mut self) {
        let Some((profile, was_new)) = self
            .laser_manager
            .as_ref()
            .map(|st| (st.draft.clone(), st.is_new))
        else {
            return;
        };
        let original_id = profile.id.clone();
        self.laser_backend.save_profile(profile);
        let saved = if was_new {
            self.laser_backend.registry.profiles.last().cloned()
        } else {
            self.laser_backend
                .registry
                .profiles
                .iter()
                .find(|profile| profile.id == original_id)
                .cloned()
        };
        if let (Some(st), Some(profile)) = (self.laser_manager.as_mut(), saved) {
            st.selected_id = Some(profile.id.clone());
            st.draft = profile;
            st.is_new = false;
        }
        self.apply_active_laser_workspace();
        self.charon_runtime
            .configure(&self.ui_settings, &self.laser_backend.registry);
        self.toasts.success("Laser-Profil gespeichert.");
    }

    pub fn laser_manager_delete(&mut self) {
        let Some(id) = self
            .laser_manager
            .as_ref()
            .and_then(|st| st.selected_id.clone())
        else {
            return;
        };
        self.laser_backend.delete_profile(&id);
        self.charon_runtime
            .configure(&self.ui_settings, &self.laser_backend.registry);
        if let Some(profile) = self.laser_backend.registry.profiles.first().cloned() {
            self.laser_manager_select(&profile.id);
        } else {
            self.laser_manager_new();
        }
        self.toasts.success("Laser-Profil gelöscht.");
    }

    pub fn laser_manager_machine_read(&mut self) {
        self.activate_managed_laser();
        match self.laser_backend.read_machine_settings() {
            Ok(values) => {
                let count = values.len();
                if let Some(st) = self.laser_manager.as_mut() {
                    st.machine_settings = values;
                    st.machine_dirty.clear();
                    st.machine_confirm_write = false;
                }
                self.toasts
                    .success(format!("{count} Ruida-Register gelesen."));
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    pub fn laser_manager_machine_write(&mut self) {
        self.activate_managed_laser();
        let changes: Vec<_> = self
            .laser_manager
            .as_ref()
            .map(|st| st.machine_dirty.iter().map(|(&a, &v)| (a, v)).collect())
            .unwrap_or_default();
        if changes.is_empty() {
            return;
        }
        match self.laser_backend.write_machine_settings(&changes) {
            Ok(values) => {
                if let Some(st) = self.laser_manager.as_mut() {
                    st.machine_settings = values;
                    st.machine_dirty.clear();
                    st.machine_confirm_write = false;
                }
                self.toasts.success(format!(
                    "{} Ruida-Register geschrieben und bestätigt.",
                    changes.len()
                ));
            }
            Err(error) => self.app_error = Some(error),
        }
    }

    fn activate_managed_laser(&mut self) {
        if let Some(id) = self
            .laser_manager
            .as_ref()
            .and_then(|state| state.selected_id.clone())
        {
            self.laser_backend.set_active(&id);
            self.apply_active_laser_workspace();
        }
    }
}
