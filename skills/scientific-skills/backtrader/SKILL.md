---
name: backtrader
description: "Python backtesting framework for trading strategies. Data feeds, brokers, analyzers, and live trading support. Strategy development with commission models, slippage, and signal-based execution."
tags: [backtesting, trading, strategy, backtest, quant-finance, python, zorai]
---
## Overview

Backtrader is a Python backtesting framework for trading strategies. Supports multiple data feeds, live trading, commission/slippage models, custom analyzers, and visualization. Well-suited for equity, futures, and crypto strategy development.

## Installation

```bash
uv pip install backtrader
```

## SMA Crossover

```python
import backtrader as bt

class SmaCross(bt.Strategy):
    params = dict(short=10, long=30)

    def __init__(self):
        sma_short = bt.ind.SMA(self.data.close, period=self.params.short)
        sma_long = bt.ind.SMA(self.data.close, period=self.params.long)
        self.crossover = bt.ind.CrossOver(sma_short, sma_long)

    def next(self):
        if self.crossover > 0:
            self.buy()
        elif self.crossover < 0:
            self.sell()

cerebro = bt.Cerebro()
data = bt.feeds.YahooFinanceData(dataname="AAPL", fromdate="2022-01-01", todate="2023-01-01")
cerebro.adddata(data)
cerebro.addstrategy(SmaCross)
cerebro.broker.setcash(10000.0)
print(f"Final value: ${cerebro.run()[0]:.2f}")
cerebro.plot()
```

## References
- [Backtrader docs](https://www.backtrader.com/docu/)
- [Backtrader GitHub](https://github.com/mementum/backtrader)