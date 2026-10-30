import { makeSecretProviderDefinitionRegistry } from "@t3tools/secret-provider-core/shared/definition";
import * as Aws from "@t3tools/secret-provider-aws/shared/definition";
import * as OnePassword from "@t3tools/secret-provider-onepassword/shared/definition";
import * as OpenBao from "@t3tools/secret-provider-openbao/shared/definition";

export const builtInSecretProviderDefinitions = makeSecretProviderDefinitionRegistry([
  OnePassword.definition,
  OpenBao.definition,
  Aws.definition,
]);
