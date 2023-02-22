use std::str::FromStr;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{error::ApiError, request::Operation};

// Policy is used to represent the policy specified by an ACL configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Policy {
    pub name: String,
    pub paths: Vec<PathPolicy>,
    pub namespace_id: String,
}

impl Policy {
    #[must_use]
    pub fn new(name: String, paths: Vec<PathPolicy>, namespace_id: String) -> Self {
        Self {
            name,
            paths,
            namespace_id,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn is_authorized(&self, path: &str, operations: &[Operation]) -> bool {
        self.paths
            .iter()
            .any(|path_policy| path_policy.is_authorized(path, operations))
    }

    #[must_use]
    pub fn batch_is_authorized(policies: &[Policy], derived_policies: &[Policy]) -> bool {
        let mut derived_policies = derived_policies
            .iter()
            // No need to check policies that have the same name
            .filter(|p| !policies.iter().any(|p2| p2.name == p.name));

        derived_policies.all(|derived_policy| {
            derived_policy.paths.iter().all(|derived_policy_path| {
                // Verify that path for policy is allowed by any of the existing policies
                policies.iter().any(|policy| {
                    policy.is_authorized(&derived_policy_path.path, &derived_policy_path.operations)
                })
            })
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct PathPolicy {
    pub path: String,
    // TODO: rename to capabilities
    pub operations: Vec<Operation>,
}

lazy_static::lazy_static! {
    static ref HCL_POLICY_REGEX: Regex = Regex::new(r#"path"(.+)"\{capabilities=\[(.+)\]\}"#).expect("a valid regex");

    static ref HCL_POLICY_RULE_REGEX: Regex = Regex::new(r"(?m)path[^\}]+\}").expect("a valid regex");
}

impl PathPolicy {
    #[must_use]
    pub fn new(path: String, operations: Vec<Operation>) -> Self {
        Self { path, operations }
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub fn operations(&self) -> &[Operation] {
        &self.operations
    }

    /// Parse a raw string into a list of policies.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not in a valid policy format.
    pub fn parse(s: &str) -> Result<Vec<Self>, ApiError> {
        // Remove comments
        let mut s = s
            .lines()
            .filter(|line| !line.trim_start().starts_with('#'))
            .collect::<String>();
        s.retain(|c| !c.is_whitespace());

        let s = s.replace('\n', "");

        let rules = HCL_POLICY_RULE_REGEX.captures_iter(&s);
        let mut policies = vec![];
        for rule in rules {
            let ma = rule.get(0).map(|m| m.range()).map(|range| &s[range]);
            if let Some(m) = ma {
                let caps = HCL_POLICY_REGEX
                    .captures(m)
                    .ok_or_else(ApiError::bad_request)?;
                let path = caps.get(1).ok_or_else(ApiError::bad_request)?.as_str();
                let operations = caps
                    .get(2)
                    .ok_or_else(ApiError::bad_request)?
                    .as_str()
                    .split(',')
                    .map(|c| c.chars().filter(|c| c.is_alphabetic()).collect::<String>())
                    .map(|s| Operation::from_str(&s))
                    .collect::<Result<Vec<_>, _>>()?;
                policies.push(PathPolicy {
                    path: path.into(),
                    operations,
                });
            }
        }

        Ok(policies)
    }

    fn is_authorized(&self, path: &str, operations: &[Operation]) -> bool {
        if self.path.ends_with('*') {
            if !path.starts_with(&self.path[..self.path.len() - 1]) {
                return false;
            }
        } else if path != self.path {
            return false;
        };

        operations.iter().all(|op| self.operations.contains(op))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use Operation::{Create, Delete, Read, Update};

    #[test]
    fn parses_policy() {
        let default_policy = r#"
        # Allow tokens to look up their own properties
path "auth/token/lookup-self" {
    capabilities = ["read"]
}

# Allow tokens to renew themselves
path "auth/token/renew-self" {
    capabilities = ["update"]
}

# Allow tokens to revoke themselves
path "auth/token/revoke-self" {
    capabilities = ["update"]
}

# Allow a token to look up its own capabilities on a path
path "sys/capabilities-self" {
    capabilities = ["update"]
}



# Allow a token to look up its resultant ACL from all policies. This is useful
# for UIs. It is an internal path because the format may change at any time
# based on how the internal ACL features and capabilities change.
path "sys/internal/ui/resultant-acl" {
    capabilities = ["read"]
}

# Allow a token to renew a lease via lease_id in the request body; old path for
# old clients, new path for newer
path "sys/renew" {
    capabilities = ["update"]
}
path "sys/leases/renew" {
    capabilities = ["update"]
}

# Allow looking up lease properties. This requires knowing the lease ID ahead
# of time and does not divulge any sensitive information.
path "sys/leases/lookup" {
    capabilities = ["update"]
}

# Allow a token to manage its own cubbyhole
path "cubbyhole/*" {
    capabilities = ["create", "read", "update", "delete"]
}

# Allow a token to wrap arbitrary values in a response-wrapping token
path "sys/wrapping/wrap" {
    capabilities = ["update"]
}

# Allow a token to look up the creation time and TTL of a given
# response-wrapping token
path "sys/wrapping/lookup" {
    capabilities = ["update"]
}

# Allow a token to unwrap a response-wrapping token. This is a convenience to
# avoid client token swapping since this is also part of the response wrapping
# policy.
path "sys/wrapping/unwrap" {
    capabilities = ["update"]
}

# Allow general purpose tools
path "sys/tools/hash" {
    capabilities = ["update"]
}
path "sys/tools/hash/*" {
    capabilities = ["update"]
}

# Allow checking the status of a Control Group request if the user has the
# accessor
path "sys/control-group/request" {
    capabilities = ["update"]
}
"#;

        let parse_result = PathPolicy::parse(default_policy);
        assert!(parse_result.is_ok());
        let policies = parse_result.unwrap();
        assert_eq!(
            policies,
            vec![
                PathPolicy {
                    path: "auth/token/lookup-self".into(),
                    operations: vec![Read],
                },
                PathPolicy {
                    path: "auth/token/renew-self".into(),
                    operations: vec![Update],
                },
                PathPolicy {
                    path: "auth/token/revoke-self".into(),
                    operations: vec![Update],
                },
                PathPolicy {
                    path: "sys/capabilities-self".into(),
                    operations: vec![Update],
                },
                PathPolicy {
                    path: "sys/internal/ui/resultant-acl".into(),
                    operations: vec![Read],
                },
                PathPolicy {
                    path: "sys/renew".into(),
                    operations: vec![Update],
                },
                PathPolicy {
                    path: "sys/leases/renew".into(),
                    operations: vec![Update],
                },
                PathPolicy {
                    path: "sys/leases/lookup".into(),
                    operations: vec![Update],
                },
                PathPolicy {
                    path: "cubbyhole/*".into(),
                    operations: vec![Create, Read, Update, Delete],
                },
                PathPolicy {
                    path: "sys/wrapping/wrap".into(),
                    operations: vec![Update],
                },
                PathPolicy {
                    path: "sys/wrapping/lookup".into(),
                    operations: vec![Update],
                },
                PathPolicy {
                    path: "sys/wrapping/unwrap".into(),
                    operations: vec![Update],
                },
                PathPolicy {
                    path: "sys/tools/hash".into(),
                    operations: vec![Update],
                },
                PathPolicy {
                    path: "sys/tools/hash/*".into(),
                    operations: vec![Update],
                },
                PathPolicy {
                    path: "sys/control-group/request".into(),
                    operations: vec![Update],
                },
            ]
        );
    }

    #[test]
    fn authorize_request_against_policy() {
        use Operation::{Read, Update};
        let policy = PathPolicy {
            path: "sys/mounts".into(),
            operations: vec![Read],
        };
        assert!(policy.is_authorized("sys/mounts", &[Read]));
        assert!(!policy.is_authorized("sys/mounts", &[Update]));
        assert!(!policy.is_authorized("sys/mounts/", &[Read]));
        assert!(!policy.is_authorized("sys/", &[Read]));
        assert!(!policy.is_authorized("secret/", &[Read]));
        assert!(!policy.is_authorized("/", &[Read]));

        let policy = PathPolicy {
            path: "sys/*".into(),
            operations: vec![Read, Update],
        };
        assert!(policy.is_authorized("sys/mounts", &[Read]));
        assert!(policy.is_authorized("sys/mounts", &[Update]));
        assert!(policy.is_authorized("sys/mounts/", &[Read]));
        assert!(policy.is_authorized("sys/", &[Read]));
        assert!(!policy.is_authorized("secret/", &[Read]));
        assert!(!policy.is_authorized("/", &[Read]));
    }
}
