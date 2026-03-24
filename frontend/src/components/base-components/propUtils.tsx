import type React from "react";
import { EditableShell } from "../EditableShell";
import type { BuilderMetaEnvelope, ViewProps } from "./shared";
import type { UINodeBuilderMeta } from "../../schemas/uiSchema";

const isPlainObject = (value: unknown): value is Record<string, unknown> =>
    value !== null && typeof value === "object" && !Array.isArray(value);

export const splitViewProps = (props: ViewProps) => {
    const {
        style,
        className,
        wrapperStyle,
        wrapperClassName,
        componentStyle,
        componentClassName,
        componentProps: legacyComponentProps,
        children,
        visible,
        hidden,
        resizable,
        resizeAxis,
        minWidth,
        minHeight,
        maxWidth,
        maxHeight,
        __nodeId,
        __viewId,
        __componentType,
        __builder,
        ...componentProps
    } = props as {
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
        __nodeId?: string;
        __viewId?: string;
        __componentType?: string;
        __builder?: UINodeBuilderMeta;
    } & Record<string, unknown>;

    const legacyComponentPropsRecord = isPlainObject(legacyComponentProps) ? legacyComponentProps : {};
    const legacyComponentStyle = isPlainObject(legacyComponentPropsRecord.style)
        ? (legacyComponentPropsRecord.style as React.CSSProperties)
        : undefined;
    const legacyComponentClassName = typeof legacyComponentPropsRecord.className === "string"
        ? legacyComponentPropsRecord.className
        : undefined;

    const resolvedWrapperStyle = wrapperStyle ?? style;
    const resolvedWrapperClassName = wrapperClassName ?? className;
    const resolvedComponentStyle = componentStyle ?? legacyComponentStyle;
    const resolvedComponentClassName = componentClassName ?? legacyComponentClassName;

    return {
        style: resolvedWrapperStyle,
        className: resolvedWrapperClassName,
        children,
        visible,
        hidden,
        resizable,
        resizeAxis,
        minWidth,
        minHeight,
        maxWidth,
        maxHeight,
        builderMeta: {
            nodeId: __nodeId,
            viewId: __viewId,
            componentType: __componentType,
            builder: __builder,
        },
        componentProps: {
            ...componentProps,
            ...legacyComponentPropsRecord,
            ...(resolvedComponentStyle !== undefined ? { style: resolvedComponentStyle } : {}),
            ...(resolvedComponentClassName ? { className: resolvedComponentClassName } : {}),
        },
    };
};

export const renderEditableWrapper = ({
    style,
    className,
    children,
    visible,
    hidden,
    resizable,
    resizeAxis,
    minWidth,
    minHeight,
    maxWidth,
    maxHeight,
    builderMeta,
    content,
}: {
    style?: React.CSSProperties;
    className?: string;
    children?: React.ReactNode;
    visible?: boolean;
    hidden?: boolean;
    resizable?: boolean;
    resizeAxis?: "both" | "horizontal" | "vertical";
    minWidth?: number | string;
    minHeight?: number | string;
    maxWidth?: number | string;
    maxHeight?: number | string;
    builderMeta?: BuilderMetaEnvelope;
    content: React.ReactNode;
}) => {
    return (
        <EditableShell
            style={style}
            className={className}
            visible={visible}
            hidden={hidden}
            resizable={resizable}
            resizeAxis={resizeAxis}
            minWidth={minWidth}
            minHeight={minHeight}
            maxWidth={maxWidth}
            maxHeight={maxHeight}
            builderNodeId={builderMeta?.nodeId}
            builderViewId={builderMeta?.viewId}
            builderComponentType={builderMeta?.componentType}
            builderMeta={builderMeta?.builder}
            content={content}
        >
            {children}
        </EditableShell>
    );
};