// SPDX-License-Identifier: AGPL-3.0-only
//! Decide whether the native relay should resume or clean up on startup.

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StartupAction {
    StayOff,
    Resume,
    Disconnect,
}

pub(crate) fn action(profile: Option<&super::store::Profile>, now_ms: u64) -> StartupAction {
    let Some(profile) = profile else {
        return StartupAction::StayOff;
    };

    if !profile.enabled() {
        return if profile.device().is_some() {
            StartupAction::Disconnect
        } else {
            StartupAction::StayOff
        };
    }

    if profile
        .device()
        .is_some_and(|device| device.expires_at <= now_ms)
    {
        StartupAction::Disconnect
    } else {
        StartupAction::Resume
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::remote_relay::store::{Profile, Store};
    use crate::remote_relay::{now_ms, DeviceCredential};

    fn configured_profile() -> (tempfile::TempDir, Store, Profile) {
        let directory = tempfile::tempdir().unwrap();
        let store = Store::in_directory(directory.path().join("relay"));
        let profile = store.configure(None, "review").unwrap();
        (directory, store, profile)
    }

    fn enabled_profile() -> (tempfile::TempDir, Store, Profile) {
        let (directory, store, profile) = configured_profile();
        let profile = store.enable(profile.revision()).unwrap();
        (directory, store, profile)
    }

    fn device(expires_at: u64) -> DeviceCredential {
        DeviceCredential {
            id: "d".repeat(64),
            management_token: "m".repeat(64),
            expires_at,
        }
    }

    fn profile_with_device(enabled: bool, expires_at: u64) -> (tempfile::TempDir, Profile) {
        let (directory, _store, profile) = if enabled {
            enabled_profile()
        } else {
            configured_profile()
        };
        let mut serialized = serde_json::to_value(&profile).unwrap();
        serialized.as_object_mut().unwrap().insert(
            "device".into(),
            serde_json::to_value(device(expires_at)).unwrap(),
        );
        let profile = serde_json::from_value(serialized).unwrap();
        (directory, profile)
    }

    #[test]
    fn missing_profile_stays_off() {
        assert_eq!(action(None, now_ms()), StartupAction::StayOff);
    }

    #[test]
    fn disabled_profile_without_device_stays_off() {
        let (_directory, _store, profile) = configured_profile();
        assert_eq!(action(Some(&profile), now_ms()), StartupAction::StayOff);
    }

    #[test]
    fn disabled_profile_with_device_disconnects() {
        let now = now_ms();
        let (_directory, profile) = profile_with_device(false, now + 60_000);
        assert_eq!(action(Some(&profile), now), StartupAction::Disconnect);
    }

    #[test]
    fn enabled_profile_without_device_can_resume_prior_consent() {
        let (_directory, _store, profile) = enabled_profile();
        assert_eq!(action(Some(&profile), now_ms()), StartupAction::Resume);
    }

    #[test]
    fn enabled_profile_with_live_device_resumes() {
        let now = now_ms();
        let (_directory, profile) = profile_with_device(true, now + 60_000);
        assert_eq!(action(Some(&profile), now), StartupAction::Resume);
    }

    #[test]
    fn enabled_profile_with_expired_device_disconnects() {
        let now = now_ms();
        let (_directory, profile) = profile_with_device(true, now - 1);
        assert_eq!(action(Some(&profile), now), StartupAction::Disconnect);
    }

    #[test]
    fn expiry_at_now_disconnects_and_does_not_modify_profile() {
        let now = now_ms();
        let (_directory, profile) = profile_with_device(true, now);
        let before = serde_json::to_string(&profile).unwrap();

        assert_eq!(action(Some(&profile), now), StartupAction::Disconnect);
        assert_eq!(serde_json::to_string(&profile).unwrap(), before);
    }
}
