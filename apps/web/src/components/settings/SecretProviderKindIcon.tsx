import { KeyRoundIcon } from "lucide-react";
import type { ComponentType } from "react";

import { AwsIcon, OnePasswordIcon, OpenBaoIcon } from "../Icons";

type KindIcon = ComponentType<{ readonly className?: string }>;

const ICONS: Readonly<Record<string, KindIcon>> = {
  one_password: OnePasswordIcon,
  openbao: OpenBaoIcon,
  aws_secrets_manager: AwsIcon,
};

/** Unknown kinds keep a generic key so a row never renders without one. */
export function secretProviderKindIcon(kind: string): KindIcon {
  return Object.hasOwn(ICONS, kind) ? ICONS[kind]! : KeyRoundIcon;
}

export function SecretProviderKindIcon({
  kind,
  className,
}: {
  readonly kind: string;
  readonly className?: string;
}) {
  switch (kind) {
    case "one_password":
      return <OnePasswordIcon className={className} aria-hidden />;
    case "openbao":
      return <OpenBaoIcon className={className} aria-hidden />;
    case "aws_secrets_manager":
      return <AwsIcon className={className} aria-hidden />;
    default:
      return <KeyRoundIcon className={className} aria-hidden />;
  }
}
