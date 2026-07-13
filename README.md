# Taskflow

Next.js、Rust、MySQL、Redisで構成した、単一ユーザー向けのタスク管理アプリです。

## 起動方法

1. `.env.example` を `.env` にコピーします。
2. 必要に応じて `FIXED_PASSWORD` を変更します。
3. `docker compose up --build` を実行します。
4. http://localhost:3000 を開きます。

初期設定のログインパスワードは `taskapp` です。

## 構成

- `frontend`: Next.js
- `backend`: Rust / Axum
- `mysql`: タスクの永続化
- `redis`: ログインセッション

## API

- `POST /api/login`
- `POST /api/logout`
- `GET /api/session`
- `GET /api/tasks`
- `POST /api/tasks`
- `PATCH /api/tasks/:id`
- `DELETE /api/tasks/:id`

この認証方式は初期開発用です。本番運用前にユーザーテーブル、パスワードハッシュ、CSRF対策、TLSを追加してください。
