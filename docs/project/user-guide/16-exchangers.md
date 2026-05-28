# Exchangers (Push-Pull and Pull-Pull)

The Exchangers feature (available in the admin panel and via CLI) provides a more powerful way to exchange mail with another server when direct federation is difficult or not possible.

This is especially useful in restricted or censored networks.

## What Are Exchangers?

An **exchanger** is an intermediary server that helps move mail between your server and the outside world (or between different restricted networks).

Instead of your server trying to talk directly to every other server, you can route some or all of your traffic through one or more trusted exchangers.

## Two Main Directions

### 1. Push (Your Server Sends to the Exchanger)

You configure your server to **push** outgoing mail to the exchanger instead of sending it directly.

- When your server has a message for `user@somewhere.com`, it sends it to the exchanger first.
- The exchanger then delivers it onward.

This is useful when your server has limited outbound connectivity.

### 2. Pull (The Exchanger Retrieves Mail From You)

The exchanger can also **pull** mail from your server on a schedule.

- You give the exchanger a special API URL on your server (for example `https://your-server/exchanger/pull`).
- The exchanger contacts this URL at regular intervals (long polling).
- If there is mail waiting for the exchanger’s users, you deliver it during the pull.
- This is often called a **pull-pull** or **push-pull** setup depending on the direction.

This mode is very useful when the exchanger cannot reach you directly (common in some regions).

## How to Configure Exchangers

### In the Admin Web Panel

There is a dedicated **Exchangers** section in the admin interface.

For each exchanger you can configure:

- Name (for your own reference)
- URL of the exchanger
- Whether it is enabled
- Poll interval (how often it should pull from you, if using pull mode)
- Last poll time (for monitoring)

You can add, edit, enable/disable, or remove exchangers from this screen.

### Via CLI

```bash
# List current exchangers
madmail exchanger list

# Add a new exchanger (push mode)
madmail exchanger add mypartner https://partner.example.com/exchanger

# Set poll interval (for pull mode, in seconds)
madmail exchanger set mypartner --poll-interval 60

# Enable or disable
madmail exchanger enable mypartner
madmail exchanger disable mypartner
```

## Push-Pull vs Pull-Pull

- **Push-Push**: Simple endpoint rewrite (see the Endpoint Rewrite guide). Your server always pushes to a fixed host.
- **Push-Pull**: Your server pushes some mail to the exchanger. The exchanger can also pull from you on demand.
- **Pull-Pull**: The exchanger primarily pulls mail from you via long polling on a specific API path. Very useful in one-way connectivity situations.

The Exchangers system in the admin panel supports the more advanced push-pull and pull-pull scenarios.

## Security and Trust

- Exchangers see the mail they handle (encrypted, of course).
- Only add exchangers you fully trust.
- The pull mechanism requires you to expose a specific API endpoint. You should protect this endpoint (usually with the admin token or a shared secret).
- Monitor the "Last Poll" time in the admin panel to detect if the exchanger stops communicating.

## Common Use Cases

- Operating in countries with heavy internet restrictions.
- Using a more reliable or better-connected server as a gateway.
- Creating redundancy (if direct federation fails, the exchanger can still deliver).
- Working with partners who can only use pull-based exchange.

## Monitoring

In the admin web panel you can see:
- Whether each exchanger is enabled
- The configured poll interval
- When it last successfully contacted you

If an exchanger stops polling, you will see it in the interface and can investigate.

## Relationship to Endpoint Rewrite

Endpoint rewrite (the simpler "push-push" feature) is often enough for basic redirection.

The full Exchangers system is more powerful because it supports:
- Bidirectional exchange
- Scheduled pulling
- Better monitoring and control

Many advanced or restricted deployments use both features together.

## Next Steps

- For the simpler redirection case, see the [Endpoint Rewrite guide](./15-endpoint-rewrite.md).
- Both features are available in the admin web panel and via the `madmail` CLI.
- Always test exchanger configurations carefully, especially pull-based setups.
