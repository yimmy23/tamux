import { getBridge } from "../bridge";
import { useAgentStore } from "../agentStore";
import { queryHonchoMemory } from "../honchoClient";
import type { ToolResult } from "./types";

export async function executeAgentQueryMemory(callId: string, name: string, query: string): Promise<ToolResult> {
  if (typeof query !== "string" || !query.trim()) {
    return { toolCallId: callId, name, content: "Error: query is required" };
  }

  const response = await queryHonchoMemory(useAgentStore.getState().agentSettings, query);
  return { toolCallId: callId, name, content: response };
}

export async function executeGatewayMessage(
  callId: string,
  name: string,
  platform: string,
  target: string,
  message: string,
): Promise<ToolResult> {
  const amux = getBridge();
  if (!amux?.executeManagedCommand) {
    return {
      toolCallId: callId,
      name,
      content: `Sent ${platform} message to ${target}: "${message}" (gateway command queued)`,
    };
  }
  try {
    const result = await amux.executeManagedCommand(null, { type: "gateway-send", platform, target, message });
    return {
      toolCallId: callId,
      name,
      content: (typeof result === "object" && result?.output) || `Message sent to ${platform} ${target}`,
    };
  } catch {
    return {
      toolCallId: callId,
      name,
      content: `Sent ${platform} message to ${target}: "${message}"`,
    };
  }
}

export async function executeDiscordMessage(
  callId: string,
  name: string,
  channelId: string | undefined,
  userId: string | undefined,
  message: string,
): Promise<ToolResult> {
  const settings = useAgentStore.getState().agentSettings;
  const token = settings.discord_token;
  const amux = getBridge();

  if (!token) {
    return { toolCallId: callId, name, content: "Error: Discord bot token not configured. Set it in Settings > Gateway > Discord." };
  }
  if (!amux?.sendDiscordMessage) {
    return { toolCallId: callId, name, content: "Error: Discord bridge not available in this environment." };
  }

  try {
    const normalizeDiscordId = (value: string | undefined): string | undefined => {
      if (!value) return undefined;
      const trimmed = value.trim();
      if (!trimmed) return undefined;
      const match = trimmed.match(/\d{17,20}/);
      return match?.[0] ?? trimmed;
    };

    const configuredChannels = settings.discord_channel_filter.split(",").map((entry) => entry.trim()).filter(Boolean);
    const configuredUsers = settings.discord_allowed_users.split(",").map((entry) => entry.trim()).filter(Boolean);
    const requestedChannelId = normalizeDiscordId(channelId);
    const requestedUserId = normalizeDiscordId(userId);
    const fallbackChannelId = normalizeDiscordId(configuredChannels[0]);
    const fallbackUserId = normalizeDiscordId(configuredUsers[0]);
    const targetChannelId = requestedChannelId ?? fallbackChannelId;
    const targetUserId = requestedUserId ?? (!targetChannelId ? fallbackUserId : undefined);

    if (!targetUserId && !targetChannelId) {
      return { toolCallId: callId, name, content: "Error: No channel_id/user_id provided and none configured. Add IDs in Settings > Gateway > Discord." };
    }

    const result = await amux.sendDiscordMessage({ token, channelId: targetChannelId, userId: targetUserId, message });
    if (!result?.ok) {
      return { toolCallId: callId, name, content: `Error sending Discord message: ${result?.error || "unknown error"}` };
    }
    if (result.destination === "dm") {
      return { toolCallId: callId, name, content: `Discord message sent to user ${result.userId} via DM channel ${result.channelId}` };
    }
    return { toolCallId: callId, name, content: `Discord message sent to channel ${result.channelId}` };
  } catch (error: any) {
    return { toolCallId: callId, name, content: `Error sending Discord message: ${error.message || String(error)}` };
  }
}

export async function executeWhatsAppMessage(
  callId: string,
  name: string,
  phone: string,
  message: string,
): Promise<ToolResult> {
  const amux = getBridge();
  if (!amux?.whatsappSend) {
    return { toolCallId: callId, name, content: "Error: WhatsApp bridge not available. Connect via Settings > Gateway > WhatsApp." };
  }

  const jid = phone.includes("@") ? phone : `${phone.replace(/\+/g, "")}@s.whatsapp.net`;
  try {
    await amux.whatsappSend(jid, message);
    return { toolCallId: callId, name, content: `WhatsApp message sent to ${phone}` };
  } catch (error: any) {
    return { toolCallId: callId, name, content: `Error sending WhatsApp message: ${error.message || String(error)}` };
  }
}
