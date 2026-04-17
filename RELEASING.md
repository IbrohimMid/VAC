# Releasing

## Branch Protection

To enforce the required CI checks on the `main` branch, run the following GitHub CLI command:

```bash
gh api \
  --method PUT \
  -H "Accept: application/vnd.github.v3+json" \
  /repos/:owner/:repo/branches/main/protection \
  -f "required_status_checks[strict]=true" \
  -f "required_status_checks[contexts][]=Lint" \
  -f "required_status_checks[contexts][]=Test (ubuntu-latest)" \
  -f "required_status_checks[contexts][]=Test (macos-latest)" \
  -f "required_status_checks[contexts][]=Test (windows-latest)" \
  -f "required_status_checks[contexts][]=Integration" \
  -f "required_status_checks[contexts][]=Cargo Deny" \
  -f "enforce_admins=false" \
  -f "required_pull_request_reviews[dismiss_stale_reviews]=true" \
  -f "restrictions=null"
```
