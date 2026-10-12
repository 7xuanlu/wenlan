// SPDX-License-Identifier: AGPL-3.0-only
import { Buildings, Lightbulb, MapPin, Shapes, User, type IconProps } from "@phosphor-icons/react";

type TopicIconProps = IconProps & { entityType?: string };

export function TopicIcon({ entityType, ...props }: TopicIconProps) {
  const type = entityType?.trim().toLowerCase();
  const Icon = type === "person" ? User
    : type === "organization" ? Buildings
    : type === "place" ? MapPin
    : type === "concept" ? Lightbulb
    : Shapes;

  return <Icon {...props} aria-hidden="true" focusable="false" weight="regular" />;
}
