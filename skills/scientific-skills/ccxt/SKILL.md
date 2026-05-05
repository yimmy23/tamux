---
name: ccxt
description: "Unified cryptocurrency exchange trading API. 100+ exchange clients (Binance, Coinbase, Kraken, Bybit, OKX). Market data, order management, websocket streaming, and arbitrage workflows."
tags: [crypto, exchange, trading, api, binance, coinbase, market-data, zorai]
---
## Overview

CCXT provides a unified API for 100+ cryptocurrency exchanges (Binance, Coinbase, Kraken, Bybit, OKX, KuCoin, Gate.io). Market data, order management, websocket streaming, and arbitrage detection through a single consistent interface across all supported exchanges.

## Installation

```bash
uv pip install ccxt
```

## Market Data

```python
import ccxt

exchange = ccxt.binance()
ticker = exchange.fetch_ticker("BTC/USDT")
print(f"Symbol: {ticker['symbol']}, Bid: {ticker['bid']}, Ask: {ticker['ask']}")

ohlcv = exchange.fetch_ohlcv("ETH/USDT", "1h", limit=100)
for candle in ohlcv:
    ts, o, h, l, c, v = candle
    print(f"Time: {ts}, O: {o:.2f}, H: {h:.2f}, L: {l:.2f}, C: {c:.2f}")
```

## Trading

```python
balance = exchange.fetch_balance()
print(f"Free USDT: {balance['free']['USDT']}")
order = exchange.create_market_buy_order("ETH/USDT", 0.1)
print(f"Order filled: {order['status']}")
```

## References
- [CCXT docs](https://docs.ccxt.com/)
- [CCXT GitHub](https://github.com/ccxt/ccxt)