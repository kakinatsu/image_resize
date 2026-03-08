# Image Upload Service – Database Schema

## 1. 概要

本システムでは、画像のメタデータ保存に **SQLite** を使用する。

画像本体は Cloudflare R2 に保存し、SQLite には以下のような管理情報のみを保存する。

* 画像ID
* R2上の保存先キー
* 出力画像の形式
* 出力画像の幅・高さ
* 出力画像のサイズ
* 作成日時
* 有効期限
* 削除日時

本システムでは **単一のテーブル `images` のみ** を使用する。
MVPの範囲では、追加テーブルは作成しない。

---

## 2. 設計方針

### 2.1 画像本体は保存しない

SQLite は画像バイナリを保存しない。
画像本体は常に Cloudflare R2 に保存する。

SQLite はメタデータのみを保持する。

---

### 2.2 永続データは最小限にする

本サービスは低コスト・低複雑性を優先するため、保存する情報は必要最小限とする。

以下のような情報は保存しない。

* 元画像バイナリ
* 元画像のEXIF
* ユーザー情報
* 認証情報
* アクセスログ
* 監査ログ

---

### 2.3 削除は `deleted_at` で管理する

期限切れ画像の削除時には、即座にレコードを物理削除せず、まず `deleted_at` を設定する。

これにより以下の利点がある。

* 削除処理の再実行がしやすい
* 実装が単純なまま安全性を少し上げられる
* 削除済み判定をアプリ側で行いやすい

MVPではこの方式を採用する。

---

### 2.4 日時は RFC3339 UTC 文字列で保存する

SQLite では日時型の扱いが弱いため、本システムでは日時を **RFC3339 UTC形式の文字列** として保存する。

例:

2026-03-06T15:00:00Z

対象カラム:

* `created_at`
* `expires_at`
* `deleted_at`

---

## 3. テーブル一覧

本システムで使用するテーブルは以下の1つのみとする。

* `images`

---

## 4. `images` テーブル

### 4.1 用途

`images` テーブルは、アップロード済み画像のメタデータを保持する。

1レコードは1画像に対応する。

---

### 4.2 カラム定義

| Column       | Type    | Not Null | Description                        |
| ------------ | ------- | -------- | ---------------------------------- |
| id           | TEXT    | Yes      | 公開用画像ID。主キー                        |
| object_key   | TEXT    | Yes      | Cloudflare R2 上のオブジェクトキー           |
| content_type | TEXT    | Yes      | 保存画像の Content-Type。常に `image/webp` |
| width        | INTEGER | Yes      | 保存画像の幅                             |
| height       | INTEGER | Yes      | 保存画像の高さ                            |
| size_bytes   | INTEGER | Yes      | 保存画像のサイズ（byte）                     |
| created_at   | TEXT    | Yes      | 作成日時（RFC3339 UTC）                  |
| expires_at   | TEXT    | Yes      | 有効期限（RFC3339 UTC）                  |
| deleted_at   | TEXT    | No       | 削除日時（RFC3339 UTC）。未削除時は NULL       |

---

### 4.3 CREATE TABLE

CREATE TABLE images (
id           TEXT PRIMARY KEY,
object_key   TEXT NOT NULL UNIQUE,
content_type TEXT NOT NULL,
width        INTEGER NOT NULL CHECK (width > 0),
height       INTEGER NOT NULL CHECK (height > 0),
size_bytes   INTEGER NOT NULL CHECK (size_bytes >= 0),
created_at   TEXT NOT NULL,
expires_at   TEXT NOT NULL,
deleted_at   TEXT NULL
);

---

### 4.4 カラム詳細

#### `id`

* 公開用の画像識別子
* `GET /i/{id}` で使用する
* 一意であること
* URL安全であること
* 推測困難であること

推奨生成方式:

* ULID

例:

01JXYZABCDEF1234567890ABCD

---

#### `object_key`

* Cloudflare R2 上で画像を保存するキー
* 外部公開URLとは別に管理する
* 一意であること

推奨形式:

images/YYYY/MM/DD/{id}.webp

例:

images/2026/03/06/01JXYZABCDEF1234567890ABCD.webp

---

#### `content_type`

* 保存画像の MIME type
* 本システムでは常に `image/webp`

MVPでは将来拡張を考慮して列を持つが、現時点では固定値として扱う。

---

#### `width`

* 保存画像の幅
* 0より大きい整数

---

#### `height`

* 保存画像の高さ
* 0より大きい整数

---

#### `size_bytes`

* 保存画像のファイルサイズ
* 0以上の整数

---

#### `created_at`

* 画像の作成日時
* アップロード完了時刻を保存する
* RFC3339 UTC 文字列

例:

2026-03-06T15:00:00Z

---

#### `expires_at`

* 画像の有効期限
* `created_at + 12時間` を保存する
* RFC3339 UTC 文字列

例:

2026-03-07T03:00:00Z

---

#### `deleted_at`

* 画像を削除済みとして扱った日時
* 未削除の間は NULL
* 削除ジョブ成功後に設定する
* RFC3339 UTC 文字列

---

## 5. インデックス

期限切れ画像の検索および削除済み判定を効率化するため、以下のインデックスを作成する。

CREATE INDEX idx_images_expires_at
ON images (expires_at);

CREATE INDEX idx_images_deleted_at
ON images (deleted_at);

---

## 6. データ操作方針

### 6.1 INSERT

画像アップロード成功時は、R2への保存完了後に `images` テーブルへ INSERT を行う。

保存する値:

* `id`
* `object_key`
* `content_type`
* `width`
* `height`
* `size_bytes`
* `created_at`
* `expires_at`
* `deleted_at = NULL`

---

### 6.2 SELECT

画像取得時は `id` をキーにレコードを検索する。

取得対象は以下条件を満たすものとする。

* `id` が一致する
* `deleted_at IS NULL`
* `expires_at` が現在時刻より未来

ただし実装上は、

1. `id` で取得
2. アプリケーション側で `deleted_at` と `expires_at` を判定

としてもよい。

---

### 6.3 UPDATE

削除ジョブによりR2上の画像削除が成功した場合、該当レコードの `deleted_at` を更新する。

例:

UPDATE images
SET deleted_at = ?
WHERE id = ?;

`?` には RFC3339 UTC の削除時刻文字列を設定する。

---

### 6.4 DELETE

MVPではレコードの物理削除は必須としない。
削除済み管理は `deleted_at` により行う。

将来的にテーブル肥大化が問題になる場合のみ、一定期間後に物理削除してもよい。

ただし現時点ではアクセス数・登録数ともに極めて少ない想定のため、レコード削除は不要とする。

---

## 7. 期限切れ画像の検索

削除ジョブは、以下の条件を満たす画像を削除対象とする。

* `expires_at` が現在時刻より過去
* `deleted_at IS NULL`

概念上のSQLは以下。

SELECT id, object_key
FROM images
WHERE expires_at < ?
AND deleted_at IS NULL;

`?` にはジョブ実行時刻の RFC3339 UTC 文字列を設定する。

---

## 8. アプリケーション上の有効判定

画像取得APIでは、以下のいずれかに該当する場合は無効とみなす。

* 対象レコードが存在しない
* `deleted_at` が NULL ではない
* 現在時刻が `expires_at` 以上

この場合、APIは `404 Not Found` を返す。

---

## 9. 初期化SQL

MVPで使用する初期化SQLは以下とする。

CREATE TABLE images (
id           TEXT PRIMARY KEY,
object_key   TEXT NOT NULL UNIQUE,
content_type TEXT NOT NULL,
width        INTEGER NOT NULL CHECK (width > 0),
height       INTEGER NOT NULL CHECK (height > 0),
size_bytes   INTEGER NOT NULL CHECK (size_bytes >= 0),
created_at   TEXT NOT NULL,
expires_at   TEXT NOT NULL,
deleted_at   TEXT NULL
);

CREATE INDEX idx_images_expires_at
ON images (expires_at);

CREATE INDEX idx_images_deleted_at
ON images (deleted_at);

---

## 10. このスキーマで意図的にやらないこと

本スキーマでは、以下は意図的に実装しない。

* ユーザーテーブルの追加
* アクセス履歴テーブルの追加
* 監査ログテーブルの追加
* 画像タグや説明文の保持
* 元画像サイズの保持
* 元ファイル名の保持
* 複数バージョン画像の保持
* 複数フォーマット保存

本サービスは一時的な画像共有のみを目的とするため、スキーマは最小構成を維持する。