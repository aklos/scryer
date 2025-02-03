https://community.fly.io/t/adding-pgvector-to-fly-postgres/12202

1. `docker build . -t prohobo34/pg-vector --platform "linux/amd64"`

2. `docker push prohobo34/pg-vector`

Launch: `fly postgres create --image-ref prohobo34/pg-vector:latest`
Deploy: `fly deploy -a blue-surf-3106 --strategy immediate --image prohobo34/pg-vector:latest --no-cache`
