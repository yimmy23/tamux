import type { ZoraiNavIconId } from "./navigation";

type IconProps = {
  icon: ZoraiNavIconId;
};

export function ZoraiNavIcon({ icon }: IconProps) {
  return (
    <svg
      className="zorai-nav-icon"
      viewBox="0 0 24 24"
      aria-hidden="true"
      focusable="false"
    >
      {renderIconPath(icon)}
    </svg>
  );
}

export function ZoraiBrandMark() {
  return (
    <div className="zorai-brand-mark" aria-hidden="true">
      <span>Z</span>
    </div>
  );
}

function renderIconPath(icon: ZoraiNavIconId) {
  if (icon === "threads") {
    return (
      <>
        <path d="M5 6.5h14v7H9l-4 4v-11Z" />
        <path d="M8.5 9h7" />
        <path d="M8.5 11.5h4.5" />
      </>
    );
  }

  if (icon === "goals") {
    return (
      <>
        <circle cx="12" cy="12" r="7" />
        <circle cx="12" cy="12" r="3" />
        <path d="M12 5v3" />
        <path d="M12 16v3" />
        <path d="M5 12h3" />
        <path d="M16 12h3" />
      </>
    );
  }

  if (icon === "workspaces") {
    return (
      <>
        <rect x="4" y="5" width="7" height="6" rx="1" />
        <rect x="13" y="5" width="7" height="6" rx="1" />
        <rect x="4" y="13" width="7" height="6" rx="1" />
        <rect x="13" y="13" width="7" height="6" rx="1" />
      </>
    );
  }

  if (icon === "tools") {
    return (
      <>
        <path d="M14.5 5.5 18 2l4 4-3.5 3.5" />
        <path d="M13 7 5 15l-1 5 5-1 8-8" />
        <path d="M6.5 16.5 7.5 17.5" />
      </>
    );
  }

  if (icon === "activity") {
    return (
      <>
        <path d="M4 13h4l2-6 4 10 2-4h4" />
        <path d="M4 19h16" />
      </>
    );
  }

  return (
    <>
      <circle cx="12" cy="12" r="3" />
      <path d="M12 3v3" />
      <path d="M12 18v3" />
      <path d="M3 12h3" />
      <path d="M18 12h3" />
      <path d="m5.6 5.6 2.1 2.1" />
      <path d="m16.3 16.3 2.1 2.1" />
      <path d="m18.4 5.6-2.1 2.1" />
      <path d="m7.7 16.3-2.1 2.1" />
    </>
  );
}

