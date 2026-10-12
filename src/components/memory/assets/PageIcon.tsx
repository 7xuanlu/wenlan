// SPDX-License-Identifier: AGPL-3.0-only
import { Notebook, type IconProps } from "@phosphor-icons/react";

export function PageIcon({ size = 20, ...props }: IconProps) {
  return <Notebook {...props} size={size} weight="regular" aria-hidden="true" focusable="false" data-resource-icon="page" />;
}
