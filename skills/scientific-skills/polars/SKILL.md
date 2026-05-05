---
name: polars
description: Fast in-memory DataFrame library for datasets that fit in RAM. Use when pandas is too slow but data still fits in memory. Lazy evaluation, parallel execution, Apache Arrow backend. Best for 1-100GB datasets, ETL pipelines, faster pandas replacement. For larger-than-RAM data use dask or vaex.
license: https://github.com/pola-rs/polars/blob/main/LICENSE
tags: [scientific-skills, polars, pandas, dask]
metadata:
    skill-author: K-Dense Inc.
--------|--------|--------|
| Select column | `df["col"]` | `df.select("col")` |
| Filter | `df[df["col"] > 10]` | `df.filter(pl.col("col") > 10)` |
| Add column | `df.assign(x=...)` | `df.with_columns(x=...)` |
| Group by | `df.groupby("col").agg(...)` | `df.group_by("col").agg(...)` |
| Window | `df.groupby("col").transform(...)` | `df.with_columns(...).over("col")` |

### Key Syntax Patterns

**Pandas sequential (slow):**
```python
df.assign(
    col_a=lambda df_: df_.value * 10,
    col_b=lambda df_: df_.value * 100
)
```

**Polars parallel (fast):**
```python
df.with_columns(
    col_a=pl.col("value") * 10,
    col_b=pl.col("value") * 100,
)
```

For comprehensive migration guide, load `references/pandas_migration.md`.

## Best Practices

### Performance Optimization

1. **Use lazy evaluation for large datasets:**
   ```python
   lf = pl.scan_csv("large.csv")  # Don't use read_csv
   result = lf.filter(...).select(...).collect()
   ```

2. **Avoid Python functions in hot paths:**
   - Stay within expression API for parallelization
   - Use `.map_elements()` only when necessary
   - Prefer native Polars operations

3. **Use streaming for very large data:**
   ```python
   lf.collect(streaming=True)
   ```

4. **Select only needed columns early:**
   ```python
   # Good: Select columns early
   lf.select("col1", "col2").filter(...)

   # Bad: Filter on all columns first
   lf.filter(...).select("col1", "col2")
   ```

5. **Use appropriate data types:**
   - Categorical for low-cardinality strings
   - Appropriate integer sizes (i32 vs i64)
   - Date types for temporal data

### Expression Patterns

**Conditional operations:**
```python
pl.when(condition).then(value).otherwise(other_value)
```

**Column operations across multiple columns:**
```python
df.select(pl.col("^.*_value$") * 2)  # Regex pattern
```

**Null handling:**
```python
pl.col("x").fill_null(0)
pl.col("x").is_null()
pl.col("x").drop_nulls()
```

For additional best practices and patterns, load `references/best_practices.md`.

## Resources

This skill includes comprehensive reference documentation:

### references/
- `core_concepts.md` - Detailed explanations of expressions, lazy evaluation, and type system
- `operations.md` - Comprehensive guide to all common operations with examples
- `pandas_migration.md` - Complete migration guide from pandas to Polars
- `io_guide.md` - Data I/O operations for all supported formats
- `transformations.md` - Joins, concatenation, pivots, and reshaping operations
- `best_practices.md` - Performance optimization tips and common patterns

Load these references as needed when users require detailed information about specific topics.

