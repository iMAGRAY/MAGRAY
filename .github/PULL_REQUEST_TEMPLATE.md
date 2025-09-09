# Pull Request - Atom IDE

## Summary
<!-- Briefly describe what this PR changes and why -->

## Type of Change
<!-- Check all that apply -->
- [ ] 🐛 Bug fix (non-breaking change that fixes an issue)
- [ ] ✨ New feature (non-breaking change that adds functionality)  
- [ ] 💥 Breaking change (fix or feature that would cause existing functionality to not work as expected)
- [ ] 📚 Documentation update
- [ ] 🔧 Configuration change (CI, tooling, etc.)
- [ ] 🔒 Security enhancement
- [ ] ♻️ Refactoring (no functional changes)

## Invariants Checklist
<!-- MANDATORY: All items must be checked before merge -->

### Repository & Toolchain
- [ ] ✅ Changes are compatible with Rust 1.82.0 (MSRV)
- [ ] ✅ `cargo clippy -- -D warnings` passes without warnings
- [ ] ✅ `cargo fmt --check` passes
- [ ] ✅ `cargo deny check` passes (licenses, advisories, bans)

### ANTI-MOCK Policy Compliance
- [ ] ✅ No mock/fake/stub patterns introduced (`cfg(feature = "mock")`, `mockall`, etc.)
- [ ] ✅ No dev-bypass patterns (`insecure-dev-signature`, `ai-mock`, `offline-fallback`)
- [ ] ✅ No `todo!()`, `unimplemented!()`, `panic!("TODO")` in production code
- [ ] ✅ External service failures result in user-facing errors (fail-closed)

### Architecture Compliance  
- [ ] ✅ No WebView usage in core UI (`crates/atom-ui`, `crates/atom-core`, `apps/atomd`)
- [ ] ✅ No Visual Studio Marketplace references (only Open VSX allowed)
- [ ] ✅ IPC messages include `request_id`, `deadline_millis` for cancellation/timeout
- [ ] ✅ Tree-sitter parsers are ABI 15 compatible (version 0.25.x)

### Security & Dependencies
- [ ] ✅ No hardcoded secrets, tokens, or credentials
- [ ] ✅ New dependencies use permissive licenses (MIT, Apache-2.0, BSD)
- [ ] ✅ WASM plugins use fuel metering and capability restrictions
- [ ] ✅ Network access goes through egress broker with allowlist

### Performance & Quality
- [ ] ✅ No blocking calls in async functions (use `spawn_blocking` for CPU work)
- [ ] ✅ Memory usage bounded (no unbounded collections or caches)
- [ ] ✅ Input latency remains ≤16ms for UI operations

## Testing
<!-- Describe how you tested your changes -->

### Manual Testing
- [ ] Tested on Windows/macOS/Linux (check all that apply)
- [ ] Verified with realistic file sizes/project structures
- [ ] Tested offline scenarios show proper error messages

### Automated Testing
- [ ] Unit tests added/updated and passing
- [ ] Integration tests cover new functionality  
- [ ] Performance benchmarks meet KPI gates (if applicable)

## Performance Impact
<!-- Required for core changes -->
- **Startup time impact**: None / +X ms / Not measured
- **Memory usage impact**: None / +X MB / Not measured  
- **Build time impact**: None / +X seconds / Not measured

## Security Considerations
<!-- Required if touching security-sensitive areas -->
- [ ] Changes reviewed for privilege escalation risks
- [ ] Sandbox/capability restrictions maintained
- [ ] No new attack surfaces introduced
- [ ] Secrets properly handled through OS keychain

## Documentation
- [ ] Code comments added/updated for complex logic
- [ ] API documentation updated (if public API changed)
- [ ] CHANGELOG.md updated (for user-facing changes)
- [ ] RFC created for architectural changes

## Breaking Changes
<!-- Only if "Breaking change" is checked above -->
**What breaks:**
<!-- Describe what functionality changes -->

**Migration path:**
<!-- How users should update their code/config -->

**Deprecation timeline:**
<!-- When old functionality will be removed -->

## Additional Notes
<!-- Any other context, screenshots, or information -->

---

### Reviewer Notes
**For maintainers:** This PR affects the following areas:
<!-- Auto-populated by CODEOWNERS, or manually list affected teams -->

**Risk level:** 🟢 Low / 🟡 Medium / 🔴 High
**Merge requirements:** 
- [ ] Minimum 2 approvals from `@atom-ide/core-team` (for core changes)
- [ ] Security team approval (for security-sensitive changes)  
- [ ] Architecture team approval (for breaking changes)