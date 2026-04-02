const { Client, GatewayIntentBits, Partials } = require('discord.js');

let discordClient = null;
let discordClientToken = null;

function normalizeDiscordSnowflake(value) {
    if (typeof value !== 'string') return null;
    const trimmed = value.trim();
    if (!trimmed) return null;
    const match = trimmed.match(/\d{17,20}/);
    return match ? match[0] : trimmed;
}

function cleanupDiscordClient() {
    if (discordClient) {
        try {
            discordClient.destroy();
        } catch {
            // Ignore cleanup errors.
        }
    }
    discordClient = null;
    discordClientToken = null;
}

async function getDiscordClient(token) {
    if (!token || typeof token !== 'string' || !token.trim()) {
        throw new Error('Discord bot token is required');
    }

    const normalizedToken = token.trim();
    if (discordClient && discordClientToken === normalizedToken && discordClient.isReady()) {
        return discordClient;
    }

    cleanupDiscordClient();

    const client = new Client({
        intents: [
            GatewayIntentBits.Guilds,
            GatewayIntentBits.GuildMessages,
            GatewayIntentBits.MessageContent,
            GatewayIntentBits.DirectMessages,
        ],
        partials: [Partials.Channel],
    });

    await new Promise((resolve, reject) => {
        let settled = false;
        const timeout = setTimeout(() => {
            if (settled) return;
            settled = true;
            reject(new Error('Discord login timeout'));
        }, 15000);

        client.once('ready', () => {
            if (settled) return;
            settled = true;
            clearTimeout(timeout);
            resolve();
        });

        client.once('error', (error) => {
            if (settled) return;
            settled = true;
            clearTimeout(timeout);
            reject(error);
        });

        client.login(normalizedToken).catch((error) => {
            if (settled) return;
            settled = true;
            clearTimeout(timeout);
            reject(error);
        });
    });

    discordClient = client;
    discordClientToken = normalizedToken;
    return client;
}

async function sendDiscordMessage(payload = {}) {
    const token = typeof payload.token === 'string' ? payload.token : '';
    const channelId = normalizeDiscordSnowflake(payload.channelId);
    const userId = normalizeDiscordSnowflake(payload.userId);
    const message = typeof payload.message === 'string' ? payload.message : '';

    if (!token.trim()) {
        return { ok: false, error: 'Discord bot token missing' };
    }
    if (!message.trim()) {
        return { ok: false, error: 'Discord message is empty' };
    }
    if (!channelId && !userId) {
        return { ok: false, error: 'No channelId or userId provided' };
    }

    try {
        const client = await getDiscordClient(token);

        if (channelId) {
            const channel = await client.channels.fetch(channelId, { force: true });
            if (!channel || !channel.isTextBased() || typeof channel.send !== 'function') {
                return { ok: false, error: `Channel ${channelId} is not text-send capable` };
            }

            const sent = await channel.send({ content: message });
            return {
                ok: true,
                destination: 'channel',
                channelId,
                messageId: sent.id,
            };
        }

        if (userId) {
            const user = await client.users.fetch(userId, { force: true });
            const dm = await user.createDM();
            const sent = await dm.send(message);
            return {
                ok: true,
                destination: 'dm',
                channelId: dm.id,
                userId: user.id,
                messageId: sent.id,
            };
        }

        return { ok: false, error: 'No resolvable Discord destination provided' };
    } catch (error) {
        const rawMessage = error && error.message ? error.message : String(error);
        const statusCode = error && typeof error.status === 'number' ? error.status : null;
        const code = error && typeof error.code !== 'undefined' ? String(error.code) : null;
        let hint = '';

        if (statusCode === 404 || code === '10003') {
            hint = ' (Discord returned Not Found: verify bot access and that channel/user IDs are valid snowflakes)';
        } else if (statusCode === 403 || code === '50013') {
            hint = ' (Discord returned Forbidden: bot lacks Send Messages permission for target channel)';
        }

        return { ok: false, error: `${rawMessage}${hint}` };
    }
}

module.exports = {
    cleanupDiscordClient,
    sendDiscordMessage,
};
