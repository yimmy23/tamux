---
name: darts
description: "Darts — time series forecasting library by Unit8. Unified API across ARIMA, Prophet, CatBoost, N-BEATS, TFT, TCN, Transformer, and RNN models. Backtesting, probabilistic forecasting, and covariate support."
tags: [darts, time-series, forecasting, deep-learning, probabilistic, backtesting, zorai]
---
## Overview

Darts (Unit8) provides a unified forecasting API across statistical models (ARIMA, Prophet, Theta), deep learning (N-BEATS, TFT, TCN, Transformer, RNN), and ensemble methods. Supports univariate/multivariate, probabilistic forecasting, covariate handling, and backtesting.

## Installation

```bash
uv pip install darts
```

## Basic Forecast

```python
from darts import TimeSeries
from darts.models import ExponentialSmoothing
import pandas as pd

series = TimeSeries.from_dataframe(pd.DataFrame({"y": [1,2,3,4,5,6,7,8,9,10]}), value_cols="y")
model = ExponentialSmoothing()
model.fit(series)
forecast = model.predict(6)
print(forecast.values())
```

## Deep Learning (N-BEATS)

```python
from darts.models import NBEATSModel
model = NBEATSModel(input_chunk_length=24, output_chunk_length=12)
model.fit(train, epochs=100)
pred = model.predict(12)
```

## Backtesting

```python
from darts.metrics import mae, mape

errors = model.backtest(series, start=0.7, forecast_horizon=6, stride=1)
print(f"MAE: {mae(errors):.3f}, MAPE: {mape(errors):.3f}")
```

## References
- [Darts docs](https://unit8co.github.io/darts/)
- [Darts GitHub](https://github.com/unit8co/darts)