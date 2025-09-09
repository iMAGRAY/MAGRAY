# RFC 0000: [Title]

- **Feature Name:** (fill me in with a unique identifier, my_awesome_feature)
- **Start Date:** YYYY-MM-DD
- **RFC PR:** [atom-ide/atom-ide#0000](https://github.com/atom-ide/atom-ide/pull/0000)
- **Implementation Issue:** [atom-ide/atom-ide#0000](https://github.com/atom-ide/atom-ide/issues/0000)
- **Author(s):** @username

## Summary

[One paragraph explanation of the feature/change.]

## Motivation

[Why are we doing this? What use cases does it support? What is the expected outcome?]

## Detailed Design

[This is the bulk of the RFC. Explain the design in enough detail for somebody familiar with Atom IDE to understand, and for somebody familiar with the implementation to implement. This should get into specifics and corner-cases, and include examples of how the feature is used.]

## Invariants Impact Analysis

> **CRITICAL**: All RFCs that modify project invariants must include this section

### Affected Invariants
<!-- Check all that apply and provide justification -->

#### Rust Toolchain (MSRV = 1.82.0)
- [ ] **No Change** - This RFC does not affect Rust version requirements
- [ ] **Maintains Compatibility** - Changes remain compatible with Rust 1.82.0  
- [ ] **Requires MSRV Bump** - New MSRV: `1.XX.0` (Requires separate RFC for MSRV change)

**Justification:** [Explain impact on Rust compatibility]

#### Technology Stack
- [ ] **No Change** - No new dependencies or technology changes
- [ ] **Slint UI (≥1.13)** - Changes affect native UI layer
- [ ] **Tree-sitter (0.25.x, ABI 15)** - Changes affect syntax parsing
- [ ] **Tantivy (0.25)** - Changes affect search/indexing  
- [ ] **Wasmtime (36.x LTS)** - Changes affect plugin system
- [ ] **Other** - [Specify technology and version requirements]

**Justification:** [Explain why these technology choices are maintained/changed]

#### Architecture Constraints
- [ ] **No WebView in Core** - This RFC maintains the prohibition of WebView in core UI
- [ ] **Open VSX Only** - This RFC maintains the restriction to Open VSX (no VS Marketplace)  
- [ ] **Process Isolation** - This RFC maintains crash-isolation between components
- [ ] **Anti-Mock Policy** - This RFC does not introduce any mock/fake/stub patterns

**Violation Justification:** [If any architecture constraint is violated, provide detailed justification and migration plan]

#### Performance Gates
- [ ] **Maintains KPI** - All performance gates continue to be met
- [ ] **Startup < 300ms** - Cold startup time remains under 300ms
- [ ] **Project Open < 200ms** - Project opening remains under 200ms to interactivity
- [ ] **Input Latency ≤ 16ms** - Input response time at 60 FPS maintained
- [ ] **Base RAM ≤ 200MB** - Memory baseline preserved

**Performance Impact:** [Quantify expected impact on KPIs, include benchmarks if available]

## Implementation Strategy

### Phase 1: [Name]
- **Duration:** X weeks
- **Success Criteria:** [Measurable outcomes]
- **Rollback Plan:** [How to undo if issues arise]

### Phase 2: [Name] (if applicable)
- **Dependencies:** [What must be completed first]
- **Success Criteria:** [Measurable outcomes]

## Compatibility Analysis

### Breaking Changes
- [ ] **None** - This is a fully backward compatible change
- [ ] **API Changes** - [List affected APIs and migration path]
- [ ] **Configuration Changes** - [List affected config and migration path]  
- [ ] **Plugin API Changes** - [Impact on WASM/Native plugins]
- [ ] **LSP Changes** - [Impact on language server integration]

### Migration Strategy
[For breaking changes, provide detailed migration instructions]

## Testing Strategy

### Automated Testing
- [ ] Unit tests for core functionality
- [ ] Integration tests for component interaction
- [ ] Performance regression tests
- [ ] Security/sandbox escape tests (if applicable)

### Manual Testing  
- [ ] Cross-platform testing (Windows/macOS/Linux)
- [ ] Large project testing (>100k files)
- [ ] Stress testing under resource constraints

## Documentation Requirements

- [ ] Update CLAUDE.md if invariants change
- [ ] Update user-facing documentation
- [ ] Update developer/contributor documentation  
- [ ] Update API documentation (if applicable)

## Security Considerations

[Address any security implications, especially if changes affect:]
- Sandbox/isolation mechanisms
- Network access/egress controls  
- File system access permissions
- Plugin capability restrictions
- Cryptographic operations
- Input validation/sanitization

## Open Questions

[What parts of the design are still unclear or need community input?]

- Question 1?
- Question 2?

## Future Possibilities  

[Think about what the natural extension and evolution of your feature might be and how this would fit into the language. This is also a good place to "dump ideas", if they are out of scope for the RFC you are writing but otherwise related.]

## Alternatives Considered

[What other designs have been considered and why weren't they chosen?]

## Implementation Timeline

| Milestone | Target Date | Owner | Dependencies |
|-----------|-------------|-------|--------------|
| RFC Approval | YYYY-MM-DD | @author | - |
| Phase 1 Complete | YYYY-MM-DD | @implementer | RFC Approval |
| Testing Complete | YYYY-MM-DD | @tester | Phase 1 |
| Documentation | YYYY-MM-DD | @doc-writer | Implementation |
| Release | YYYY-MM-DD | @release-manager | All above |

## Approval Criteria

### Required Approvals
- [ ] **Architecture Team** (@atom-ide/architects) - For design and invariants  
- [ ] **Security Team** (@atom-ide/security) - If security implications
- [ ] **Core Team** (@atom-ide/core-team) - For implementation impact

### Success Metrics
- [ ] All CI checks pass including invariant validation
- [ ] Performance benchmarks meet/exceed current KPIs  
- [ ] Zero regressions in existing functionality
- [ ] Documentation complete and reviewed

---

## Implementation Checklist
<!-- To be filled out during implementation -->

- [ ] Implementation PR submitted
- [ ] All tests passing  
- [ ] Documentation updated
- [ ] Performance benchmarks updated
- [ ] Security review completed (if applicable)
- [ ] Cross-platform testing completed
- [ ] Ready for release