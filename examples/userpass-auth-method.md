# Enable sign-in with username and password

## Unseal Covert

```sh
covert operator init
covert operator unseal --unseal-keys "<key1>,<key2>,<key3>"
```

## Setup entity and policy
```sh
covert entity add --name john

covert policy add --name admin --policy "path \"sys/*\" { capabilities = [\"read\",\"update\",\"create\"] }"

covert policy list

covert entity attach-policy -n john -p admin
```

## Enable userpass auth method
```sh
covert auth enable -n userpass -p auth/userpass/
```

## Map userpass users to covert entities

```sh
# Add user to the auth method
covert userpass add --username john --password john --mount auth/userpass/

# List users
covert userpass list -m auth/userpass/

# Connect user with covert entity
covert entity attach-alias --name john --alias john --mount auth/userpass/

# Login with that user to the auth method
covert TODO
```

## Disable auth method

```sh
# Disable
covert auth disable -p auth/userpass/

# Sign in not working
covert TODO

# Existing tokens has been revoked
covert TODO
```