import type React from "react";
import type { UINodeBuilderMeta } from "../../schemas/uiSchema";

export interface HeaderProps {
    title?: string;
    description?: string;
}

export interface ButtonProps {
    label?: string;
    style?: React.CSSProperties;
    command?: string;
    variant?: "primary" | "secondary" | "danger";
}

export interface InputProps {
    placeholder?: string;
    type?: string;
    name?: string;
    command?: string;
}

export interface TextProps {
    value?: string;
    as?: React.ElementType;
}

export interface TextAreaProps {
    placeholder?: string;
    name?: string;
    rows?: number;
    command?: string;
    defaultValue?: string;
}

export interface SelectOption {
    label: string;
    value: string;
}

export interface SelectProps {
    name?: string;
    value?: string;
    options?: SelectOption[];
    command?: string;
}

export interface SpacerProps {
    size?: number | string;
}

export interface UnknownProps {
    type?: string;
}

export interface ViewMountProps {
    targetViewId?: string;
}

export interface MissionDeckProps {
    style?: React.CSSProperties;
    className?: string;
    children?: React.ReactNode;
    missionTagLabel?: string;
    missionButtonLabel?: string;
    vaultButtonLabel?: string;
    providerLabelPrefix?: string;
    approvalsLabel?: string;
    traceLabel?: string;
    opsLabel?: string;
    recallLabel?: string;
    snapshotsLabel?: string;
    missionCommand?: string;
    vaultCommand?: string;
}

export type ViewProps = {
    style?: React.CSSProperties;
    className?: string;
    wrapperStyle?: React.CSSProperties;
    wrapperClassName?: string;
    componentStyle?: React.CSSProperties;
    componentClassName?: string;
    componentProps?: Record<string, unknown>;
    children?: React.ReactNode;
    visible?: boolean;
    hidden?: boolean;
    resizable?: boolean;
    resizeAxis?: "both" | "horizontal" | "vertical";
    minWidth?: number | string;
    minHeight?: number | string;
    maxWidth?: number | string;
    maxHeight?: number | string;
} & Record<string, unknown>;

export interface BuilderMetaEnvelope {
    nodeId?: string;
    viewId?: string;
    componentType?: string;
    builder?: UINodeBuilderMeta;
}