import type { FlatSecretProviderKind } from "@t3tools/contracts";
import type { SVGProps } from "react";

import { AwsIcon, OnePasswordIcon, OpenBaoIcon } from "../Icons";

export function SecretProviderKindIcon({
  kind,
  ...props
}: SVGProps<SVGSVGElement> & { readonly kind: FlatSecretProviderKind }) {
  switch (kind) {
    case "1password":
      return <OnePasswordIcon {...props} />;
    case "openbao":
      return <OpenBaoIcon {...props} />;
    case "aws-secrets-manager":
      return <AwsIcon {...props} />;
  }
}
