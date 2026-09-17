# GitHub repository setup (Veyro)

This document describes how [WtekSupport/veyro](https://github.com/WtekSupport/veyro) is configured for open source and protected `main`.

## Repository links

| Item | URL |
|------|-----|
| Source | https://github.com/WtekSupport/veyro |
| Releases | https://github.com/WtekSupport/veyro/releases |
| Website (canonical) | https://veyrotext.vercel.app |
| Landing repo | https://github.com/WtekSupport/veyroland |
| Security | https://github.com/WtekSupport/veyro/security |

## Collaborators

Keep **Collaborators** empty for a personal repo. External contributors use **fork + pull request**. Only the owner can push to `main` by default.

## Branch protection (`main`)

Configured via GitHub (classic protection + ruleset **Protect main**):

- Required status checks: **`frontend`** and **`rust`** (strict)
- No force-push, no branch deletion
- Ruleset bypass: repository owner only

Re-apply after changing CI job names:

```powershell
powershell -ExecutionPolicy Bypass -File scripts/protect-main-branch.ps1
```

Requires `GITHUB_TOKEN`, `GH_TOKEN`, or Git Credential Manager (same credentials as `git push`).

### CI check names

Workflow [`.github/workflows/ci.yml`](../.github/workflows/ci.yml) defines jobs `frontend` and `rust`. If GitHub shows required checks as `CI / frontend` and `CI / rust`, update the ruleset in **Settings → Rules → Rulesets → Protect main** to match, or adjust contexts in `scripts/protect-main-branch.ps1`.

Verify on a test PR: both checks must pass before merge.

## Personal vs organization repos

**Named user push restrictions** (only specific GitHub users may push) are available on **organization** repositories, not personal accounts. For this repo, access control is: no collaborators + protected `main` + CI required.

## Recommended GitHub Settings (manual)

- **Actions:** require approval for first-time contributors (fork PRs)
- **Dependabot:** enabled via [`.github/dependabot.yml`](../.github/dependabot.yml)
- **Wiki:** disabled if unused (reduces spam)
- **Topics:** `tauri`, `whisper`, `voice-dictation`, `rust`, `desktop-app`, `mit`
- **Description / Website:** short tagline + https://veyrotext.vercel.app

## Contributors sidebar cache

GitHub’s homepage **Contributors** list can lag after history rewrites. Toggle **default branch** away and back in Settings if stale avatars remain. Do not add `Co-authored-by:` bot trailers (enforced by `githooks/`).
