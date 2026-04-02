use std::collections::HashMap;

use anyhow::Result;
use hyprbridge_config::AppConfig;
use hyprbridge_protocol::{DeviceGroup, TrustRecord};

use crate::provider::SyncData;

/// Conflict resolver for merging sync data from multiple sources.
///
/// Implements smart merge strategies for different data types:
/// - Config: local priority, preserve remote new fields
/// - Trust: dedupe by DeviceId, keep latest
/// - Groups: dedupe by ID, merge members
#[derive(Debug, Clone)]
pub struct ConflictResolver;

impl Default for ConflictResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl ConflictResolver {
    pub fn new() -> Self {
        Self
    }

    /// Merge two config records. Local takes priority for existing fields,
    /// but remote's new fields that local doesn't have are preserved.
    pub fn merge_config(&self, local: &AppConfig, remote: &AppConfig) -> AppConfig {
        let mut merged = local.clone();

        // Merge cors_origins: union of both
        let mut cors = local.cors_origins.clone();
        for origin in &remote.cors_origins {
            if !cors.contains(origin) {
                cors.push(origin.clone());
            }
        }
        merged.cors_origins = cors;

        // Merge topology edges: local wins for same local_edge, remote adds new ones
        let mut edges = local.topology.edges.clone();
        for remote_edge in &remote.topology.edges {
            if !edges.iter().any(|e| e.local_edge == remote_edge.local_edge) {
                edges.push(remote_edge.clone());
            }
        }
        merged.topology.edges = edges;

        // Keep local dead_zone_px if set, otherwise use remote
        if local.topology.dead_zone_px == 12 {
            // default value
            merged.topology.dead_zone_px = remote.topology.dead_zone_px;
        }

        merged
    }

    /// Merge trust records. Dedupe by device_id, keep the latest paired_at.
    pub fn merge_trust(&self, local: &[TrustRecord], remote: &[TrustRecord]) -> Vec<TrustRecord> {
        let mut map: HashMap<String, TrustRecord> = HashMap::new();

        // Insert local records
        for record in local {
            map.insert(
                record.identity.device_id.0.clone(),
                record.clone(),
            );
        }

        // Merge remote records, keeping latest
        for record in remote {
            let device_id = record.identity.device_id.0.clone();
            match map.get(&device_id) {
                Some(existing) if existing.paired_at >= record.paired_at => {
                    // Keep existing (local is newer or same)
                }
                _ => {
                    // Remote is newer or doesn't exist locally
                    map.insert(device_id, record.clone());
                }
            }
        }

        map.into_values().collect()
    }

    /// Merge device groups. Dedupe by ID, merge member lists (union).
    pub fn merge_groups(
        &self,
        local: &[DeviceGroup],
        remote: &[DeviceGroup],
    ) -> Vec<DeviceGroup> {
        let mut map: HashMap<String, DeviceGroup> = HashMap::new();

        // Insert local groups
        for group in local {
            map.insert(group.id.clone(), group.clone());
        }

        // Merge remote groups
        for group in remote {
            match map.get_mut(&group.id) {
                Some(existing) => {
                    // Merge members: union of both lists
                    for member in &group.members {
                        if !existing.members.contains(member) {
                            existing.members.push(member.clone());
                        }
                    }
                    // Keep the later created_at if remote is newer
                    if group.created_at > existing.created_at {
                        existing.name = group.name.clone();
                        existing.created_at = group.created_at;
                    }
                }
                None => {
                    // New group from remote
                    map.insert(group.id.clone(), group.clone());
                }
            }
        }

        map.into_values().collect()
    }

    /// Merge two SyncData records. Returns the merged result.
    pub fn merge(&self, local: &SyncData, remote: &SyncData) -> Result<SyncData> {
        match (local, remote) {
            (SyncData::Config(local_cfg), SyncData::Config(remote_cfg)) => {
                Ok(SyncData::Config(self.merge_config(local_cfg, remote_cfg)))
            }
            (SyncData::TrustStore(local_bytes), SyncData::TrustStore(remote_bytes)) => {
                let local_records: Vec<TrustRecord> =
                    serde_json::from_slice(local_bytes).unwrap_or_default();
                let remote_records: Vec<TrustRecord> =
                    serde_json::from_slice(remote_bytes).unwrap_or_default();
                let merged = self.merge_trust(&local_records, &remote_records);
                let bytes = serde_json::to_vec(&merged)?;
                Ok(SyncData::TrustStore(bytes))
            }
            (SyncData::DeviceGroups(local_bytes), SyncData::DeviceGroups(remote_bytes)) => {
                let local_groups: Vec<DeviceGroup> =
                    serde_json::from_slice(local_bytes).unwrap_or_default();
                let remote_groups: Vec<DeviceGroup> =
                    serde_json::from_slice(remote_bytes).unwrap_or_default();
                let merged = self.merge_groups(&local_groups, &remote_groups);
                let bytes = serde_json::to_vec(&merged)?;
                Ok(SyncData::DeviceGroups(bytes))
            }
            _ => {
                anyhow::bail!("cannot merge different SyncData variants")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, TimeZone, Utc};
    use hyprbridge_protocol::{DeviceId, DeviceIdentity, DeviceRole};

    fn make_trust_record(device_id: &str, paired_at: DateTime<Utc>) -> TrustRecord {
        TrustRecord {
            identity: DeviceIdentity {
                device_id: DeviceId(device_id.into()),
                name: format!("Device {device_id}"),
                role: DeviceRole::Controller,
                public_key: "0000000000000000000000000000000000000000000000000000000000000000".into(),
            },
            paired_at,
            note: format!("paired with {device_id}"),
        }
    }

    fn make_group(id: &str, name: &str, members: Vec<&str>) -> DeviceGroup {
        DeviceGroup {
            id: id.into(),
            name: name.into(),
            members: members.into_iter().map(|m| DeviceId(m.into())).collect(),
            created_at: Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
        }
    }

    #[test]
    fn merge_trust_dedupes_by_device_id() {
        let resolver = ConflictResolver::new();

        let local = vec![
            make_trust_record("dev-a", Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()),
            make_trust_record("dev-b", Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()),
        ];

        let remote = vec![
            make_trust_record("dev-b", Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap()),
            make_trust_record("dev-c", Utc.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap()),
        ];

        let merged = resolver.merge_trust(&local, &remote);

        assert_eq!(merged.len(), 3);

        let dev_b = merged.iter().find(|r| r.identity.device_id.0 == "dev-b").unwrap();
        assert_eq!(dev_b.paired_at, Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap());
    }

    #[test]
    fn merge_trust_keeps_local_if_newer() {
        let resolver = ConflictResolver::new();

        let local = vec![
            make_trust_record("dev-a", Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap()),
        ];

        let remote = vec![
            make_trust_record("dev-a", Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()),
        ];

        let merged = resolver.merge_trust(&local, &remote);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].paired_at, Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap());
    }

    #[test]
    fn merge_groups_dedupes_by_id_and_merges_members() {
        let resolver = ConflictResolver::new();

        let local = vec![
            make_group("grp-1", "Local Group", vec!["dev-a", "dev-b"]),
        ];

        let remote = vec![
            make_group("grp-1", "Remote Group", vec!["dev-b", "dev-c"]),
            make_group("grp-2", "Remote Only", vec!["dev-d"]),
        ];

        let merged = resolver.merge_groups(&local, &remote);

        assert_eq!(merged.len(), 2);

        let grp1 = merged.iter().find(|g| g.id == "grp-1").unwrap();
        assert_eq!(grp1.members.len(), 3);
        assert!(grp1.members.iter().any(|m| m.0 == "dev-a"));
        assert!(grp1.members.iter().any(|m| m.0 == "dev-b"));
        assert!(grp1.members.iter().any(|m| m.0 == "dev-c"));

        let grp2 = merged.iter().find(|g| g.id == "grp-2").unwrap();
        assert_eq!(grp2.members.len(), 1);
    }

    #[test]
    fn merge_groups_keeps_newer_name() {
        let resolver = ConflictResolver::new();

        let mut local = make_group("grp-1", "Old Name", vec!["dev-a"]);
        local.created_at = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();

        let mut remote = make_group("grp-1", "New Name", vec!["dev-b"]);
        remote.created_at = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();

        let merged = resolver.merge_groups(&[local], &[remote]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].name, "New Name");
    }
}
