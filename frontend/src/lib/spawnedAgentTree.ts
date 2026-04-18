import type { AgentTaskStatus } from "./agentTaskQueue";

export interface SpawnedAgentTreeSource {
    id: string;
    task_id?: string | null;
    status: AgentTaskStatus;
    created_at: number;
    thread_id?: string | null;
    parent_task_id?: string | null;
    parent_thread_id?: string | null;
}

export interface SpawnedAgentTreeNode<T extends SpawnedAgentTreeSource> {
    item: T;
    children: SpawnedAgentTreeNode<T>[];
    openable: boolean;
    live: boolean;
}

export interface SpawnedAgentTree<T extends SpawnedAgentTreeSource> {
    activeThreadId: string;
    anchor: SpawnedAgentTreeNode<T> | null;
    roots: SpawnedAgentTreeNode<T>[];
}

type SpawnedAgentTreeIndexes<T extends SpawnedAgentTreeSource> = {
    byThreadId: Map<string, T[]>;
    byParentTaskId: Map<string, T[]>;
    byParentThreadId: Map<string, T[]>;
};

function getTaskIdentity<T extends SpawnedAgentTreeSource>(item: T): string {
    return item.task_id ?? item.id;
}

function compareSpawnedAgentTreeItems<T extends SpawnedAgentTreeSource>(left: T, right: T): number {
    if (left.created_at !== right.created_at) {
        return right.created_at - left.created_at;
    }

    const leftIdentity = getTaskIdentity(left);
    const rightIdentity = getTaskIdentity(right);
    if (leftIdentity !== rightIdentity) {
        return leftIdentity.localeCompare(rightIdentity);
    }

    return left.id.localeCompare(right.id);
}

function uniqueByTaskIdentity<T extends SpawnedAgentTreeSource>(items: readonly T[]): T[] {
    const seen = new Set<string>();
    const result: T[] = [];
    for (const item of items) {
        const identity = getTaskIdentity(item);
        if (seen.has(identity)) {
            continue;
        }
        seen.add(identity);
        result.push(item);
    }
    return result;
}

function sortByPreferredOrder<T extends SpawnedAgentTreeSource>(items: readonly T[]): T[] {
    return uniqueByTaskIdentity(items).sort(compareSpawnedAgentTreeItems);
}

function canonicalizeByIdentity<T extends SpawnedAgentTreeSource>(items: readonly T[]): T[] {
    const byIdentity = new Map<string, T>();

    for (const item of items) {
        const identity = getTaskIdentity(item);
        const current = byIdentity.get(identity);
        if (!current || compareSpawnedAgentTreeItems(item, current) < 0) {
            byIdentity.set(identity, item);
        }
    }

    return sortByPreferredOrder([...byIdentity.values()]);
}

function buildIndexes<T extends SpawnedAgentTreeSource>(items: readonly T[]): SpawnedAgentTreeIndexes<T> {
    const byThreadId = new Map<string, T[]>();
    const byParentTaskId = new Map<string, T[]>();
    const byParentThreadId = new Map<string, T[]>();

    const push = (map: Map<string, T[]>, key: string, item: T) => {
        const bucket = map.get(key);
        if (bucket) {
            bucket.push(item);
            return;
        }
        map.set(key, [item]);
    };

    for (const item of items) {
        if (item.thread_id) {
            push(byThreadId, item.thread_id, item);
        }
        if (item.parent_task_id) {
            push(byParentTaskId, item.parent_task_id, item);
        }
        if (item.parent_thread_id) {
            push(byParentThreadId, item.parent_thread_id, item);
        }
    }

    for (const map of [byThreadId, byParentTaskId, byParentThreadId]) {
        for (const [key, bucket] of map.entries()) {
            map.set(key, sortByPreferredOrder(bucket));
        }
    }

    return { byThreadId, byParentTaskId, byParentThreadId };
}

function hasResolvedParent<T extends SpawnedAgentTreeSource>(item: T, identityLookup: ReadonlySet<string>): boolean {
    return Boolean(item.parent_task_id && identityLookup.has(item.parent_task_id));
}

function isSpawnedAgentTreeTerminal(status: AgentTaskStatus): boolean {
    return status === "completed" || status === "failed" || status === "cancelled";
}

function pickAnchorCandidate<T extends SpawnedAgentTreeSource>(
    activeThreadItems: readonly T[],
    identityLookup: ReadonlySet<string>,
): T | null {
    const topLevelActiveThreadItems = activeThreadItems.filter(
        (item) => !hasResolvedParent(item, identityLookup),
    );
    return topLevelActiveThreadItems[0] ?? activeThreadItems[0] ?? null;
}

function buildChildren<T extends SpawnedAgentTreeSource>(
    item: T,
    indexes: SpawnedAgentTreeIndexes<T>,
    rootIdentityLookup: ReadonlySet<string>,
    ancestry: Set<string>,
): SpawnedAgentTreeNode<T>[] {
    const currentIdentity = getTaskIdentity(item);
    const directChildren = indexes.byParentTaskId.get(currentIdentity) ?? [];
    const fallbackChildren = item.thread_id ? indexes.byParentThreadId.get(item.thread_id) ?? [] : [];
    const childCandidates = sortByPreferredOrder([...directChildren, ...fallbackChildren]).filter(
        (candidate) => !rootIdentityLookup.has(getTaskIdentity(candidate)) && !ancestry.has(getTaskIdentity(candidate)),
    );

    if (childCandidates.length === 0) {
        return [];
    }

    const nextAncestry = new Set(ancestry);
    nextAncestry.add(currentIdentity);

    return childCandidates.map((candidate) => ({
        item: candidate,
        children: buildChildren(candidate, indexes, rootIdentityLookup, nextAncestry),
        openable: Boolean(candidate.thread_id),
        live: !isSpawnedAgentTreeTerminal(candidate.status),
    }));
}

function buildNode<T extends SpawnedAgentTreeSource>(
    item: T,
    indexes: SpawnedAgentTreeIndexes<T>,
    rootIdentityLookup: ReadonlySet<string>,
): SpawnedAgentTreeNode<T> {
    return {
        item,
        children: buildChildren(item, indexes, rootIdentityLookup, new Set([getTaskIdentity(item)])),
        openable: Boolean(item.thread_id),
        live: !isSpawnedAgentTreeTerminal(item.status),
    };
}

export function deriveSpawnedAgentTree<T extends SpawnedAgentTreeSource>(
    items: readonly T[],
    activeThreadId: string | null | undefined,
): SpawnedAgentTree<T> | null {
    if (!activeThreadId || items.length === 0) {
        return null;
    }

    const canonicalItems = canonicalizeByIdentity(items);
    const indexes = buildIndexes(canonicalItems);
    const identityLookup = new Set(canonicalItems.map((item) => getTaskIdentity(item)));
    const activeThreadItems = indexes.byThreadId.get(activeThreadId) ?? [];
    const anchorCandidate = pickAnchorCandidate(activeThreadItems, identityLookup);

    const visibleRootCandidates = sortByPreferredOrder([
        ...(anchorCandidate ? [anchorCandidate] : []),
        ...(indexes.byParentThreadId.get(activeThreadId) ?? []).filter(
            (item) => !hasResolvedParent(item, identityLookup),
        ),
    ]);

    if (visibleRootCandidates.length === 0) {
        return null;
    }
    const rootIdentityLookup = new Set(visibleRootCandidates.map((item) => getTaskIdentity(item)));

    return {
        activeThreadId,
        anchor: anchorCandidate ? buildNode(anchorCandidate, indexes, rootIdentityLookup) : null,
        roots: visibleRootCandidates
            .filter((item) => !anchorCandidate || getTaskIdentity(item) !== getTaskIdentity(anchorCandidate))
            .map((item) => buildNode(item, indexes, rootIdentityLookup)),
    };
}
