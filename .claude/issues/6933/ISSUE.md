# #6933 — INC-2026-11-14-21: Five tracked paths have no row in `_audit-owners.md`

**Source**: `docs/audits/AUDIT_INCREMENTAL_2026-11-14.md` (INC-21). Verified against HEAD `77eb7e6c0` at publish time.

- **Severity**: LOW
- **Location**: `.claude/commands/_audit-owners.md`
- **Changed in**: not introduced in this window; the audit-skill re-sync (`922072af8`) rewrote the map without closing the gap

## Description
`git ls-files | .claude/commands/_audit-route.sh | grep unrouted` lists `.claude/settings.json`, `.gitattributes`, `.gitignore`, `LICENSE` and `screenshot_1790471263.png`. The PNG is a 167 KB stray screenshot committed at the repo root in `de808add3` (2026-09-26). A change to `.claude/settings.json` (permissions, hooks) routes to no audit.

## Suggested Fix
Add a repo-root-config row (owner `audit-tooling`, LOW) covering `.claude/settings.json` and the git config files; `git rm` the stray screenshot.

## Completeness Checks
- [ ] **VALIDATE**: `.claude/commands/_audit-validate.sh` passes after the owners edit
- [ ] **ROUTE**: `git ls-files | .claude/commands/_audit-route.sh | grep -c unrouted` is 0
