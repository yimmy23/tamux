# Rust Diagnostic Triage

## Common Families

- **E0277 trait bound not satisfied**: Put the bound on the function that needs it, derive or implement the trait deliberately, or convert to a type that already satisfies the contract.
- **E0308 mismatched types**: Check whether the mismatch is ownership, reference level, generic inference, or error type conversion.
- **E0382 use after move**: Borrow instead of moving, reorder operations, clone at an ownership boundary, or redesign the API to consume only when needed.
- **E0499/E0502 multiple borrows**: Shorten borrow scopes, split structs, stage reads before writes, or move mutation into a method.
- **E0521 borrowed data escapes**: Use owned data for spawned tasks, callbacks, thread boundaries, and `'static` futures.
- **E0597 value does not live long enough**: Return owned data, extend the owner lifetime, or avoid storing references in long-lived structs.

## Repair Patterns

- Introduce a local variable to end a borrow before mutation.
- Use `Option::take` to move out of a field while leaving a valid state behind.
- Replace broad `&mut self` helper calls with helpers that take only the fields they need.
- Use `Arc` for shared ownership across tasks and threads, but keep interior mutability narrow.
- Convert error types at layer boundaries with `From` or explicit mapping.

## Examples

### Split A Broad Mutable Borrow

```rust
// Avoid: helper borrows all of self even though it only needs two fields.
self.update_indexes(message)?;

// Prefer: make dependencies explicit.
update_indexes(&mut self.by_id, &mut self.by_thread, message)?;
```

### Convert Error Types At A Boundary

```rust
impl From<serde_json::Error> for SnapshotError {
    fn from(source: serde_json::Error) -> Self {
        Self::InvalidJson { source }
    }
}
```

## Anti-Patterns

- Adding `.clone()` before identifying the ownership boundary.
- Wrapping everything in `Arc<Mutex<_>>` instead of designing ownership.
- Adding lifetime parameters to structs that could own their data.
- Silencing Clippy before checking whether the lint indicates a real contract problem.
