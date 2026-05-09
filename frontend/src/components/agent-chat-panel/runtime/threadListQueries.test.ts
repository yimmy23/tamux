import { describe, expect, it, vi } from "vitest";
import { fetchHydratedRemoteThreads } from "./threadListQueries";

describe("fetchHydratedRemoteThreads", () => {
    it("passes the daemon agent filter through and hydrates lightweight threads", async () => {
        const agentListThreads = vi.fn(async (options?: { agentFilter?: string | null }) => {
            expect(options).toEqual({ agentFilter: "rarog" });
            return [
                {
                    id: "daemon-rarog-thread",
                    title: "Rarog conversation",
                    agent_name: "Rarog",
                    created_at: 1,
                    updated_at: 10,
                    messages: [],
                    total_message_count: 0,
                },
            ];
        });

        const threads = await fetchHydratedRemoteThreads({
            agentListThreads,
            fallbackAgentName: "Svarog",
            agentFilter: "rarog",
        });

        expect(agentListThreads).toHaveBeenCalledTimes(1);
        expect(threads).toHaveLength(1);
        expect(threads[0].daemonThreadId).toBe("daemon-rarog-thread");
        expect(threads[0].agent_name).toBe("Rarog");
    });

    it("falls back to the provided agent name when the daemon omits ownership", async () => {
        const agentListThreads = vi.fn(async () => [
            {
                id: "daemon-svarog-thread",
                title: "Main conversation",
                created_at: 1,
                updated_at: 20,
                messages: [],
                total_message_count: 0,
            },
        ]);

        const threads = await fetchHydratedRemoteThreads({
            agentListThreads,
            fallbackAgentName: "Svarog",
        });

        expect(threads).toHaveLength(1);
        expect(threads[0].agent_name).toBe("Svarog");
    });
});