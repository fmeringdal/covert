# Store static key value secrets

## Unseal Covert

```sh
covert operator init
covert operator unseal --unseal-keys "<key1>,<key2>,<key3>"
```

## Configure versioned KV secret engine
```sh
covert secrets enable -n kv -p kv/
```

## Manage static secrets
```sh
# Add secret
covert kv add -k example-key -d "aws_access_key=123" -d "aws_secret_access_key=456" -m kv/ 

# Add new version of same key with extra region
covert kv add -k example-key -d "aws_access_key=123" -d "aws_secret_access_key=456" -d "aws_region=us-west-1" -m kv/ 

# Read secret
covert kv read -k example-key -m kv/

# soft delete version 2
covert kv delete -k example-key -v 2 -m kv/

# Read secret version 2 is empty
covert kv read -k example-key -v 2 -m kv/

# Read secret version 1 is *not* empty
covert kv read -k example-key -v 1 -m kv/

# Recover version 2
covert kv recover -k example-key -m kv/ -v 2

# And version 2 is back
covert kv read -k example-key -v 2 -m kv/

# Hard delete
covert kv hard-delete -k example-key -v 2 -m kv/

# Recover not possible
covert kv recover -k example-key -m kv/ -v 2
```