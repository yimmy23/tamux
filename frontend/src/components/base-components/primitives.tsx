import React from "react";
import { executeCommand } from "../../registry/commandRegistry";
import { useViewBuilderStore } from "../../lib/viewBuilderStore";
import { renderEditableWrapper, splitViewProps } from "./propUtils";
import type {
  ButtonProps,
  HeaderProps,
  InputProps,
  SelectProps,
  SpacerProps,
  TextAreaProps,
  TextProps,
  UnknownProps,
  ViewProps,
} from "./shared";

export const Container: React.FC<ViewProps> = (props) => {
  const {
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
  } = splitViewProps(props);

  return renderEditableWrapper({
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
    content: null,
  });
};

export const Header: React.FC<ViewProps> = (props) => {
  const {
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
    componentProps,
  } = splitViewProps(props);
  const { title, description } = componentProps as HeaderProps;

  return renderEditableWrapper({
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
    content: (
      <div>
        <h1>{title}</h1>
        {description ? <p>{description}</p> : null}
      </div>
    ),
  });
};

export const Text: React.FC<ViewProps> = (props) => {
  const { style, className, children, visible, hidden, builderMeta, componentProps } = splitViewProps(props);
  const { value, as = "span" } = componentProps as TextProps;
  const Tag = as as React.ElementType;

  return renderEditableWrapper({
    style,
    className,
    children,
    visible,
    hidden,
    builderMeta,
    content: <Tag>{value}</Tag>,
  });
};

export const Button: React.FC<ViewProps> = (props) => {
  const { style, className, children, visible, hidden, builderMeta, componentProps } = splitViewProps(props);
  const isEditMode = useViewBuilderStore((state) => state.isEditMode);
  const { label, command, variant = "primary" } = componentProps as ButtonProps;

  return renderEditableWrapper({
    style,
    className,
    children,
    visible,
    hidden,
    builderMeta,
    content: (
      <button
        type="button"
        className={`btn-${variant}`}
        onClick={() => {
          if (!isEditMode && command) {
            void executeCommand(command);
          }
        }}
      >
        {label}
      </button>
    ),
  });
};

export const Input: React.FC<ViewProps> = (props) => {
  const { style, className, children, visible, hidden, builderMeta, componentProps } = splitViewProps(props);
  const isEditMode = useViewBuilderStore((state) => state.isEditMode);
  const { placeholder, type = "text", name, command } = componentProps as InputProps;

  return renderEditableWrapper({
    style,
    className,
    children,
    visible,
    hidden,
    builderMeta,
    content: (
      <input
        type={type}
        placeholder={placeholder}
        name={name}
        onBlur={() => {
          if (!isEditMode && command) {
            void executeCommand(command);
          }
        }}
      />
    ),
  });
};

export const TextArea: React.FC<ViewProps> = (props) => {
  const { style, className, children, visible, hidden, builderMeta, componentProps } = splitViewProps(props);
  const isEditMode = useViewBuilderStore((state) => state.isEditMode);
  const { placeholder, name, rows = 4, command, defaultValue } = componentProps as TextAreaProps;

  return renderEditableWrapper({
    style,
    className,
    children,
    visible,
    hidden,
    builderMeta,
    content: (
      <textarea
        placeholder={placeholder}
        name={name}
        rows={rows}
        defaultValue={defaultValue}
        onBlur={() => {
          if (!isEditMode && command) {
            void executeCommand(command);
          }
        }}
      />
    ),
  });
};

export const Select: React.FC<ViewProps> = (props) => {
  const { style, className, children, visible, hidden, builderMeta, componentProps } = splitViewProps(props);
  const isEditMode = useViewBuilderStore((state) => state.isEditMode);
  const { name, value, options = [], command } = componentProps as SelectProps;

  return renderEditableWrapper({
    style,
    className,
    children,
    visible,
    hidden,
    builderMeta,
    content: (
      <select
        name={name}
        defaultValue={value}
        onChange={() => {
          if (!isEditMode && command) {
            void executeCommand(command);
          }
        }}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
    ),
  });
};

export const Divider: React.FC<ViewProps> = (props) => {
  const { style, className, children, visible, hidden, builderMeta } = splitViewProps(props);

  return renderEditableWrapper({
    style,
    className,
    children,
    visible,
    hidden,
    builderMeta,
    content: <div className={["amux-divider", "amux-divider--subtle"].concat(className ? [className] : []).join(" ")} />,
  });
};

export const Spacer: React.FC<ViewProps> = (props) => {
  const { style, className, children, visible, hidden, builderMeta, componentProps } = splitViewProps(props);
  const { size = 16 } = componentProps as SpacerProps;

  return renderEditableWrapper({
    style: {
      width: size,
      height: size,
      flexShrink: 0,
      ...(style ?? {}),
    },
    className,
    children,
    visible,
    hidden,
    builderMeta,
    content: null,
  });
};

export const UnknownComponent: React.FC<UnknownProps> = ({ type }) => (
  <div style={{ color: "red", border: "1px solid red", padding: "10px" }}>
    Unknown Component: {type ?? "(missing type)"}
  </div>
);
