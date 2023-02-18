# Store static key value secrets

## Unseal Covert

```sh
covert operator init --shares 1 --threshold 1
covert operator unseal --unseal-keys "<key1>"
# Export the root token received after unseal to your environment
export COVERT_TOKEN=<TOKEN>
```

## Configure versioned KV secret engine
```sh
covert secrets enable kv --path kv/
```

## Manage static secrets
```sh
# Add secret
covert kv put example-key -d "aws_access_key=123" -d "aws_secret_access_key=456" --path kv/ 

# Add new version of same key with extra region
covert kv put example-key -d "aws_access_key=123" -d "aws_secret_access_key=456" -d "aws_region=us-west-1" --path kv/ 

# Read secret
covert kv get example-key --path kv/

# soft delete version 2
covert kv delete example-key -v 2 --path kv/

# Read secret version 2 is empty
covert kv get example-key -v 2 --path kv/

# Read secret version 1 is *not* empty
covert kv get example-key -v 1 --path kv/

# Recover version 2
covert kv recover example-key -v 2 --path kv/

# And version 2 is back
covert kv get example-key -v 2 --path kv/

# Hard delete
covert kv hard-delete example-key -v 2 --path kv/

# Recover not possible
covert kv recover example-key -v 2 --path kv/
```