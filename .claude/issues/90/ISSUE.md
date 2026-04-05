# Issue #90: Safety: no lock ordering for ad-hoc multi-query (N>2) or resources

**State:** OPEN
**Labels:** bug, ecs
**Domain:** ecs

query_2_mut enforces TypeId-sorted lock ordering, but ad-hoc combinations of >2 component
queries or any resource_mut calls have zero ordering enforcement. Currently safe (sequential
scheduler), but becomes a deadlock risk under parallel dispatch.

Fix: Provide query_N_mut builder or runtime lock-order validator. Add resource_2_mut API.
Add debug-mode thread-local tracker for resource locks.
