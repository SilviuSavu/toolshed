---
name: test-rule
description: A test rule for integration testing
type: guardrail
severity: error
scope: global
---
# No Force Push

Never use `git push --force` on main or master branches.

This can destroy other people's work and is irreversible.
