// SPDX-License-Identifier: AGPL-3.0-only
import type { TextareaHTMLAttributes } from "react";
import "./inlineTextField.css";

/** An always editable text surface; its mirror keeps wrapping and height stable. */
export function InlineTextField({ className = "", value, placeholder, ...props }: Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "value" | "defaultValue"> & { readonly value: string }) {
  return <span className={`inline-text-field ${className}`}>
    <span aria-hidden="true" className="inline-text-field-measure">{value || placeholder || " "}{"\n"}</span>
    <textarea {...props} value={value} placeholder={placeholder} rows={1} />
  </span>;
}
