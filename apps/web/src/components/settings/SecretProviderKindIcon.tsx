import type { SecretProviderKind } from "@t3tools/contracts";
import type { SVGProps } from "react";

import { OnePasswordIcon, OpenBaoIcon } from "../Icons";

export function SecretProviderKindIcon({
  kind,
  ...props
}: SVGProps<SVGSVGElement> & { readonly kind: SecretProviderKind }) {
  return kind === "1password" ? <OnePasswordIcon {...props} /> : <OpenBaoIcon {...props} />;
}
