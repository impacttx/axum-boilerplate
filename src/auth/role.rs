use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};

/// System roles; mirrors the rows of the `roles` table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Customer,
    Sales,
    Operations,
    Marketing,
    Admin,
}

impl Role {
    pub const ALL: [Role; 5] = [Role::Customer, Role::Sales, Role::Operations, Role::Marketing, Role::Admin];

    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Customer => "customer",
            Role::Sales => "sales",
            Role::Operations => "operations",
            Role::Marketing => "marketing",
            Role::Admin => "admin",
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Role {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Role::ALL.into_iter().find(|r| r.as_str() == s).ok_or_else(|| format!("unknown role: {s}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_str() {
        for r in Role::ALL {
            assert_eq!(r.as_str().parse::<Role>().unwrap(), r);
        }
        assert!("root".parse::<Role>().is_err());
    }
}
