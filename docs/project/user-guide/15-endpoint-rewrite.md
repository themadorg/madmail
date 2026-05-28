# Endpoint Rewrite (Push-Push)

Endpoint rewrite (also visible in the admin panel as "Endpoint Cache" or "Endpoint Rewrite") lets you change where your server tries to deliver mail for a specific domain.

## The Problem It Solves

Sometimes you want to send mail to `user@a.com`, but `a.com` is not directly reachable from your server. This can happen because of:

- Network restrictions
- Firewalls or censorship
- The other server only accepts mail from certain intermediaries
- Temporary routing problems

Instead of failing, you can tell your server:  
**"When someone tries to reach a.com, actually send the mail to b.com instead."**

Then `b.com` is responsible for getting the message to the final recipient on `a.com`.

This is often called a **push-push** setup because your server pushes the mail to an intermediary, which then pushes it onward.

## How to Configure It

### Via Admin Web Panel

Look for the **Endpoint Cache** or **Endpoint Rewrite** section in the admin interface.

You can add rules like:

- Lookup key: `a.com`
- Target host: `b.com` (or an IP address)
- Optional comment

### Via CLI

```bash
# Set a rewrite rule
madmail endpoint-cache set a.com b.com --comment "Route via our partner"

# List current rules
madmail endpoint-cache list

# Remove a rule
madmail endpoint-cache remove a.com
```

## How It Works Internally

When your server wants to deliver a message to `user@a.com`:

1. It checks the endpoint cache / rewrite rules first.
2. If there is a rule for `a.com`, it uses the configured target host (`b.com`) instead of doing normal MX lookup or direct federation to `a.com`.
3. It sends the message to `b.com` using the normal federation methods (HTTPS `/mxdeliv` preferred, with SMTP fallback).
4. `b.com` then takes responsibility for delivering it to the real recipient on `a.com`.

This is transparent to your users — they still address mail to `@a.com`.

## Common Use Cases

- Sending mail into restricted networks via a trusted intermediary.
- Using a more reliable or faster "gateway" server for certain domains.
- Workarounds during temporary outages or DNS problems.
- In some regions, using a known good exchanger to improve deliverability.

## Security Considerations

- The target host (`b.com`) will see the full encrypted message (just like any other federation partner).
- Only add rewrite rules for hosts you trust.
- The rule is stored in your database (`dns_overrides` table) and takes effect immediately after reload.

## Limitations

- This is mainly for **outbound** delivery from your server.
- It does not automatically make `a.com` users able to reach you (incoming traffic).
- It is a relatively simple domain → host mapping. For more advanced intermediary setups, see the Exchangers feature.

## Next Steps

- For more advanced intermediary configurations (including pull mechanisms), see the [Exchangers guide](./16-exchangers.md).
- You can also manage these rules through the admin web interface under the Endpoint Cache section.
