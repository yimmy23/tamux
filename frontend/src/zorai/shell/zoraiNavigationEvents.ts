import type { ZoraiToolId } from "../features/tools/tools";
import type { ZoraiViewId } from "./navigation";

export const ZORAI_NAVIGATE_EVENT = "zorai-navigate";

export type ZoraiReturnTarget = {
  view: ZoraiViewId;
  label: string;
};

export type ZoraiNavigateDetail = {
  view?: ZoraiViewId;
  tool?: ZoraiToolId;
  returnTarget?: ZoraiReturnTarget | null;
  goalRunId?: string | null;
};

export function navigateZorai(detail: ZoraiNavigateDetail) {
  window.dispatchEvent(new CustomEvent<ZoraiNavigateDetail>(ZORAI_NAVIGATE_EVENT, { detail }));
}
