---
name: web-quality-audit
description: Comprehensive web quality audit covering performance, accessibility, SEO, and best practices. Use when asked to "audit my site", "review web quality", "run lighthouse audit", "check page quality", or "optimize my website".
license: MIT
tags: [gsd-2, skills, web-quality-audit, performance, marketing, compliance]
metadata:
  author: web-quality-skills
  version: "1.0"
----|-------------|--------|
| **Critical** | Security vulnerabilities, complete failures | Fix immediately |
| **High** | Core Web Vitals failures, major a11y barriers | Fix before launch |
| **Medium** | Performance opportunities, SEO improvements | Fix within sprint |
| **Low** | Minor optimizations, code quality | Fix when convenient |

## Audit output format

When performing an audit, structure findings as:

```markdown
## Audit results

### Critical issues (X found)
- **[Category]** Issue description. File: `path/to/file.js:123`
  - **Impact:** Why this matters
  - **Fix:** Specific code change or recommendation

### High priority (X found)
...

### Summary
- Performance: X issues (Y critical)
- Accessibility: X issues (Y critical)
- SEO: X issues
- Best Practices: X issues

### Recommended priority
1. First fix this because...
2. Then address...
3. Finally optimize...
```

## Quick checklist

### Before every deploy
- [ ] Core Web Vitals passing
- [ ] No accessibility errors (axe/Lighthouse)
- [ ] No console errors
- [ ] HTTPS working
- [ ] Meta tags present

### Weekly review
- [ ] Check Search Console for issues
- [ ] Review Core Web Vitals trends
- [ ] Update dependencies
- [ ] Test with screen reader

### Monthly deep dive
- [ ] Full Lighthouse audit
- [ ] Performance profiling
- [ ] Accessibility audit with real users
- [ ] SEO keyword review

## References

For detailed guidelines on specific areas:
- [Core Web Vitals](../core-web-vitals/SKILL.md)
- [Accessibility](../accessibility/SKILL.md)
- [Best Practices](../best-practices/SKILL.md)
