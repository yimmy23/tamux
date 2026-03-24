import type { TerminalNotification } from "../../lib/types";
import { Badge, Button, cn, panelSurfaceClassName } from "../ui";
import { formatTime } from "./shared";

export function NotificationList({
  notifications,
  markRead,
  onSelectNotification,
  onApproveNotification,
  onDenyNotification,
}: {
  notifications: TerminalNotification[];
  markRead: (id: string) => void;
  onSelectNotification?: (notification: TerminalNotification) => void;
  onApproveNotification?: (notification: TerminalNotification) => void;
  onDenyNotification?: (notification: TerminalNotification) => void;
}) {
  return (
    <div className="flex-1 overflow-auto bg-[var(--panel)]/25 px-[var(--space-3)] py-[var(--space-3)]">
      {notifications.length === 0 ? (
        <div className="flex min-h-[16rem] items-center justify-center px-[var(--space-6)] text-center text-[var(--text-sm)] text-[var(--text-secondary)]">
          No notifications
        </div>
      ) : (
        <div className="grid gap-[var(--space-3)]">
          {notifications.map((notification) => {
            const isApproval = notification.source === "approval";
            return (
              <div
                key={notification.id}
                onClick={() => {
                  markRead(notification.id);
                  onSelectNotification?.(notification);
                }}
                className={cn(
                  panelSurfaceClassName,
                  "cursor-pointer rounded-[var(--radius-lg)] px-[var(--space-4)] py-[var(--space-3)] shadow-none transition-colors hover:bg-[var(--muted)]/70",
                  notification.isRead ? "opacity-70" : "",
                  isApproval
                    ? "border-[var(--approval-border)] bg-[var(--approval-soft)]/60"
                    : "bg-[var(--card)]/80"
                )}
              >
                <div className="flex items-start gap-[var(--space-2)]">
                  {!notification.isRead ? (
                    <span className="mt-[0.35rem] h-[0.45rem] w-[0.45rem] rounded-full bg-[var(--accent)]" />
                  ) : null}

                  <div className="min-w-0 flex-1">
                    <div className="flex flex-wrap items-center gap-[var(--space-2)]">
                      <span
                        className={cn(
                          "text-[var(--text-sm)] text-[var(--text-primary)]",
                          notification.isRead ? "font-medium" : "font-semibold"
                        )}
                      >
                        {notification.title}
                      </span>
                      <Badge variant={isApproval ? "approval" : notification.isRead ? "default" : "accent"}>
                        {isApproval ? "approval" : notification.isRead ? "read" : "new"}
                      </Badge>
                      <span className="ml-auto text-[var(--text-xs)] text-[var(--text-muted)]">
                        {formatTime(notification.timestamp)}
                      </span>
                    </div>

                    {notification.body ? (
                      <div className="mt-[var(--space-2)] whitespace-pre-wrap break-words text-[var(--text-xs)] leading-6 text-[var(--text-secondary)]">
                        {notification.body}
                      </div>
                    ) : null}

                    {notification.progress !== null ? (
                      <div className="mt-[var(--space-3)] overflow-hidden rounded-[var(--radius-full)] border border-[var(--border-subtle)] bg-[var(--muted)]">
                        <div
                          className="h-[0.35rem] bg-[var(--accent)] transition-[width] duration-300 ease-out"
                          style={{ width: `${notification.progress}%` }}
                        />
                      </div>
                    ) : null}

                    {isApproval ? (
                      <div className="mt-[var(--space-3)] flex flex-wrap gap-[var(--space-2)]">
                        <Button
                          type="button"
                          variant="primary"
                          size="sm"
                          onClick={(event) => {
                            event.stopPropagation();
                            markRead(notification.id);
                            onApproveNotification?.(notification);
                          }}
                        >
                          Allow (y)
                        </Button>
                        <Button
                          type="button"
                          variant="destructive"
                          size="sm"
                          onClick={(event) => {
                            event.stopPropagation();
                            markRead(notification.id);
                            onDenyNotification?.(notification);
                          }}
                        >
                          Deny (n)
                        </Button>
                      </div>
                    ) : null}
                  </div>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
