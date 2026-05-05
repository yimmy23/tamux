---
name: stripe-payments
description: "Stripe payments integration: charges, subscriptions, invoices, webhooks, Connect platform, and fraud prevention (Radar). Build payment workflows, recurring billing, and marketplace payouts."
tags: [stripe, payments, billing, subscriptions, fintech, api, zorai]
---
## Overview

Stripe handles payment processing for online businesses — one-time charges, subscription billing, invoicing, marketplace payouts (Connect), and fraud prevention (Radar). This skill covers the Python SDK (`stripe` module).

## Installation

```bash
uv pip install stripe
```

## One-Time Charge

```python
import stripe
stripe.api_key = "sk_test_..."

charge = stripe.Charge.create(
    amount=2000,  # $20.00 in cents
    currency="usd",
    source="tok_visa",  # token from Stripe.js
    description="Example charge",
)
print(charge.id, charge.status)
```

## Subscription

```python
customer = stripe.Customer.create(email="customer@example.com")
subscription = stripe.Subscription.create(
    customer=customer.id,
    items=[{"price": "price_123"}],  # recurring price ID
)
print(subscription.id, subscription.status)
```

## Webhook Verification

```python
from flask import request
import stripe

payload = request.data
sig_header = request.headers.get("Stripe-Signature")
event = stripe.Webhook.construct_event(payload, sig_header, "whsec_...")

if event.type == "payment_intent.succeeded":
    payment_intent = event.data.object
    print(f"Payment {payment_intent.id} succeeded")
```

## Connect Platform (Marketplace Payouts)

```python
# Create a connected account
account = stripe.Account.create(
    type="express",
    country="US",
    email="seller@example.com",
)

# Transfer funds
transfer = stripe.Transfer.create(
    amount=1000,
    currency="usd",
    destination=account.id,
)
```

## Error Handling

```python
try:
    charge = stripe.Charge.create(amount=-1, currency="usd")
except stripe.error.InvalidRequestError as e:
    print(f"Invalid request: {e.user_message}")
except stripe.error.CardError as e:
    print(f"Card declined: {e.error.decline_code}")
except stripe.error.RateLimitError:
    print("Rate limited — retry with exponential backoff")
```

## References
- [Stripe Python SDK docs](https://stripe.com/docs/api?lang=python)
- [Stripe webhook best practices](https://stripe.com/docs/webhooks)
- [Stripe Connect docs](https://stripe.com/docs/connect)