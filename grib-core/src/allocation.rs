//! Shared limit enforcement and fallible vector allocation.

use crate::{Error, Result};

/// Enforce an optional caller-configured resource limit.
pub fn ensure_limit(what: &'static str, requested: usize, limit: Option<usize>) -> Result<()> {
    if let Some(limit) = limit {
        if requested > limit {
            return Err(Error::LimitExceeded {
                what,
                requested,
                limit,
            });
        }
    }
    Ok(())
}

/// Allocate and initialize a vector while reporting allocation failure.
pub fn filled_vec<T: Clone>(len: usize, value: T, what: &'static str) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values
        .try_reserve_exact(len)
        .map_err(|error| Error::allocation(format!("{what} values"), len, error))?;
    values.resize(len, value);
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_limits_are_inclusive() {
        assert!(ensure_limit("items", 10, Some(10)).is_ok());
        assert!(matches!(
            ensure_limit("items", 11, Some(10)),
            Err(Error::LimitExceeded {
                what: "items",
                requested: 11,
                limit: 10
            })
        ));
        assert!(ensure_limit("items", usize::MAX, None).is_ok());
    }

    #[test]
    fn filled_vector_has_requested_contents() {
        assert_eq!(filled_vec(3, 7_u8, "test").unwrap(), vec![7, 7, 7]);
    }
}
