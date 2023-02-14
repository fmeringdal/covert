use chrono::{DateTime, Duration, Utc};

use crate::mount::MountConfig;

/// Calculate a new TTL
///
/// # Errors
///
/// Returns error if it fails to covert any time to live parameter to either
/// [`chrono::Duration`] or [`std::time::Duration`].
pub fn calculate_ttl(
    now: DateTime<Utc>,
    issued_at: DateTime<Utc>,
    mount_config: &MountConfig,
    ttl: Option<std::time::Duration>,
) -> Result<Duration, String> {
    let ttl = ttl.unwrap_or(mount_config.default_lease_ttl);

    let ttl = Duration::from_std(ttl).map_err(|_| "Unable to create TTL from renew response")?;

    let max_lease_ttl = Duration::from_std(mount_config.max_lease_ttl)
        .map_err(|_| "Unable to create max lease TTL from mount")?;

    let max_expires_at = issued_at + max_lease_ttl;
    let new_expires_at = now + ttl;

    let ttl = if new_expires_at > max_expires_at {
        if max_expires_at > now {
            max_expires_at - now
        } else {
            Duration::zero()
        }
    } else if new_expires_at > now {
        new_expires_at - now
    } else {
        Duration::zero()
    };
    Ok(ttl)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ttl_calculation() {
        let mount_config = MountConfig {
            default_lease_ttl: std::time::Duration::from_secs(30),
            max_lease_ttl: std::time::Duration::from_secs(3600),
        };

        fn calculate_ttl_std(
            now: DateTime<Utc>,
            issued_at: DateTime<Utc>,
            mount_config: &MountConfig,
            ttl: Option<std::time::Duration>,
        ) -> std::time::Duration {
            calculate_ttl(now, issued_at, mount_config, ttl)
                .unwrap()
                .to_std()
                .unwrap()
        }

        let mut now = Utc::now();
        let issued_at = now;

        // Default to the mount default lease ttl
        assert_eq!(
            calculate_ttl_std(now, issued_at, &mount_config, None),
            mount_config.default_lease_ttl
        );

        // Explicit ttl
        assert_eq!(
            calculate_ttl_std(
                now,
                issued_at,
                &mount_config,
                Some(std::time::Duration::from_secs(10))
            ),
            std::time::Duration::from_secs(10)
        );

        // Is capped at mount max lease ttl
        assert_eq!(
            calculate_ttl_std(
                now,
                issued_at,
                &mount_config,
                Some(mount_config.max_lease_ttl + std::time::Duration::from_secs(1))
            ),
            mount_config.max_lease_ttl
        );

        // Is capped at mount max lease ttl, when default to default mount lease ttl
        now += Duration::from_std(mount_config.max_lease_ttl).unwrap();
        assert_eq!(
            calculate_ttl_std(now, issued_at, &mount_config, None),
            std::time::Duration::ZERO
        );

        // Can not expand when max lease ttl is reached
        assert_eq!(
            now,
            issued_at + Duration::from_std(mount_config.max_lease_ttl).unwrap()
        );
        assert_eq!(
            calculate_ttl_std(
                now,
                issued_at,
                &mount_config,
                Some(std::time::Duration::from_secs(10))
            ),
            std::time::Duration::ZERO
        );
        assert_eq!(
            calculate_ttl_std(now, issued_at, &mount_config, None),
            std::time::Duration::ZERO
        );

        // Can not expand when max lease ttl is passed
        now += Duration::minutes(5);
        assert!(now > issued_at + Duration::from_std(mount_config.max_lease_ttl).unwrap());
        assert_eq!(
            calculate_ttl_std(
                now,
                issued_at,
                &mount_config,
                Some(std::time::Duration::from_secs(10))
            ),
            std::time::Duration::ZERO
        );
        assert_eq!(
            calculate_ttl_std(now, issued_at, &mount_config, None),
            std::time::Duration::ZERO
        );
    }
}
