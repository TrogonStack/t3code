//! Which ways to reach an environment, in order, mirroring `packages/client-runtime`'s
//! `connection/resolver.ts`: the direct endpoint first (a saved session, 3 s to answer), the T3
//! Connect relay as the fallback for an environment the relay lists while signed in.

use std::time::Duration;

/// How long the direct route gets before the relay is tried, as in the resolver.
pub const DIRECT_TIMEOUT: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Direct,
    Relay,
}

pub fn select(saved_direct: bool, relay_listed: bool, signed_in: bool) -> Vec<Route> {
    let mut routes = Vec::new();
    if saved_direct {
        routes.push(Route::Direct);
    }
    if relay_listed && signed_in {
        routes.push(Route::Relay);
    }
    routes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_saved_pairing_is_tried_direct_before_the_relay() {
        assert_eq!(select(true, true, true), [Route::Direct, Route::Relay]);
    }

    #[test]
    fn a_relay_only_environment_needs_the_account() {
        assert_eq!(select(false, true, true), [Route::Relay]);
        assert_eq!(select(false, true, false), []);
    }

    #[test]
    fn a_saved_pairing_stays_direct_when_signed_out() {
        assert_eq!(select(true, true, false), [Route::Direct]);
        assert_eq!(select(true, false, true), [Route::Direct]);
    }
}
