---
name: fredapi
description: "Federal Reserve Economic Data (FRED) API client. 800,000+ US and international economic time series: GDP, inflation, unemployment, interest rates, industrial production. Direct data access for macro research."
tags: [fred, economic-data, macro-economics, federal-reserve, time-series, api, zorai]
---
## Overview

Fred API provides access to Federal Reserve Economic Data (FRED) — 800,000+ US and international economic time series. Use it for GDP, unemployment, inflation, interest rates, industrial production, and financial market data.

## Installation

```bash
uv pip install fredapi
```

## Basic Usage

```python
from fredapi import Fred

fred = Fred(api_key="YOUR_API_KEY")  # get free key from research.stlouisfed.org

# Get GDP data (series ID: GDP)
gdp = fred.get_series("GDP")
print(gdp.tail())

# Get unemployment rate
unemp = fred.get_series("UNRATE")
print(unemp.tail())
```

## Search and Metadata

```python
# Search for series
results = fred.search("consumer price index", limit=5)
print(results[["id", "title", "observation_start", "observation_end"]])

# Get series info
info = fred.get_series_info("CPIAUCSL")  # CPI Urban Consumers
print(info)
```

## Multiple Series

```python
# Get multiple series at once
cpi = fred.get_series("CPIAUCSL")
fed_funds = fred.get_series("FEDFUNDS")
ten_year = fred.get_series("DGS10")

import pandas as pd
combined = pd.DataFrame({"CPI": cpi, "FedFunds": fed_funds, "10Y": ten_year})
```

## References
- [FRED API docs](https://fred.stlouisfed.org/docs/api/fred/)
- [FRED series search](https://fred.stlouisfed.org/)