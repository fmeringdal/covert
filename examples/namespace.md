# Covert tenant isolation with namespaces

## Unseal Covert

```sh
covert operator init --shares 1 --threshold 1
covert operator unseal --unseal-keys "<key1>"
# Export the root token received after unseal to your environment
export COVERT_TOKEN=<TOKEN>
```

## Create namespace
```sh
# Create tutorial namespace under the root namespace (default namespace)
covert ns create tutorial
# List namespaces under root
covert ns list
# Switch namespace context to the new namespace
export COVERT_NAMESPACE="root/tutorial"
```

## Setup entity and policy in new namespace
```sh
# Create entity that will be admin for the new namespace
covert entity add --name tutorial-admin

# This policy allows all actions 
covert policy add --name admin --policy "path \"*\" { capabilities = [\"read\",\"update\",\"create\",\"delete\"] }"

# Give admin policy to tutorial-admin entity
covert entity attach-policy --name tutorial-admin --policies admin
```

## Enable userpass auth method
```sh
covert auth enable userpass -p auth/userpass/
```

## Map userpass users to covert entities

```sh
# Add user to the auth method
covert userpass add --username tutorial-admin --password secret --path auth/userpass/

# Connect user with covert entity
covert entity attach-alias --name tutorial-admin --alias tutorial-admin --path auth/userpass/

# Login with that user to the auth method
covert userpass login --username tutorial-admin --password secret --path auth/userpass/

# Export token received in previous command
export COVERT_TOKEN=<TOKEN>

# "tutorial-admin" will be able to issue any command in the current namespace. E.g. create a KV secrets engine.
covert secrets enable kv --path kv/

# But "tutorial-admin" will not be able to issue any commands against the root namespace
export COVERT_NAMESPACE="root"

# Trying to issue the same command in the root namespace will not be authorized
covert secrets enable kv --path kv/
```
