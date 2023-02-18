# Enable sign-in with username and password

## Unseal Covert

```sh
covert operator init --shares 1 --threshold 1
covert operator unseal --unseal-keys "<key1>"
# Export the root token received after unseal to your environment
export COVERT_TOKEN=<TOKEN>
```

## Setup entity and policy
```sh
covert entity add --name john

covert policy add --name admin --policy "path \"sys/*\" { capabilities = [\"read\",\"update\",\"create\"] }"

covert policy list

covert entity attach-policy --name john --policies admin
```

## Enable userpass auth method
```sh
covert auth enable userpass -p auth/userpass/
```

## Map userpass users to covert entities

```sh
# Add user to the auth method
covert userpass add --username john --password john --path auth/userpass/

# List users
covert userpass list auth/userpass/

# Connect user with covert entity
covert entity attach-alias --name john --alias john --path auth/userpass/

# Login with that user to the auth method
covert userpass login --username john --password john --path auth/userpass/

# Export token received in previous command
export COVERT_TOKEN=<TOKEN>
```

## Disable auth method

```sh
# Disable also revokes all leases associated with the auth method
covert auth disable auth/userpass/
```