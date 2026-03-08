# Image Upload Service – API仕様

## 1. 概要

本ドキュメントは、画像アップロードサービスのHTTP API仕様を定義する。

本APIは以下の機能を提供する。

* 画像アップロード
* 保存済み画像の取得
* サーバ稼働確認

画像はアップロード後にリサイズされ、WebP形式へ変換されて保存される。
画像は一定時間後に期限切れとなり、以降はアクセスできなくなる。

---

# 2. Base URL

すべてのAPIは同一ホスト上で提供される。

例:

[https://example.com](https://example.com)

---

# 3. エンドポイント一覧

| Method | Path        | 説明       |
| ------ | ----------- | -------- |
| POST   | /api/images | 画像アップロード |
| GET    | /i/{id}     | 画像取得     |
| GET    | /healthz    | ヘルスチェック  |

---

# 4. 画像アップロード

## Endpoint

POST /api/images

---

## Query Parameters

| Name       | Type    | Required | Default | Description |
| ---------- | ------- | -------- | ------- | ----------- |
| max_width  | integer | optional | 2048    | 出力画像の最大幅    |
| max_height | integer | optional | 2048    | 出力画像の最大高さ   |

### 制約

max_width と max_height は以下の条件を満たす必要がある。

1 <= max_width <= 4096
1 <= max_height <= 4096

制約違反の場合は **400 Bad Request** を返す。

---

## Request

Content-Type:

multipart/form-data

フォームフィールド:

| Name | Type   | Required | Description |
| ---- | ------ | -------- | ----------- |
| file | binary | yes      | アップロードする画像  |

---

## 対応入力形式

以下の画像形式のみ受け付ける。

* image/jpeg
* image/png
* image/webp

以下は受け付けない。

* GIF
* SVG
* その他未対応形式

---

## 最大アップロードサイズ

10 MB

これを超える場合は

413 Payload Too Large

を返す。

---

## 画像処理ルール

サーバは以下のルールに従い画像を処理する。

1. EXIF orientation を適用する
2. アスペクト比を維持する
3. max_width と max_height の範囲内に収める
4. 元画像より拡大しない

縮小率は次の式で決定する。

scale = min(max_width / src_width, max_height / src_height, 1.0)

出力解像度は以下。

dst_width = floor(src_width × scale)
dst_height = floor(src_height × scale)

---

## 出力画像形式

すべての画像は **WebP形式** に変換される。

Content-Type:

image/webp

EXIFメタデータは保存されない。

---

## Response

成功時:

HTTP Status

201 Created

Response Body

{
"id": "01JXYZABCDEF1234567890ABCD",
"url": "[https://example.com/i/01JXYZABCDEF1234567890ABCD](https://example.com/i/01JXYZABCDEF1234567890ABCD)",
"expires_at": "2026-03-07T03:15:00Z",
"width": 1200,
"height": 675,
"content_type": "image/webp",
"size_bytes": 182344
}

---

## Response Fields

| Field        | Type    | Description   |
| ------------ | ------- | ------------- |
| id           | string  | 画像ID          |
| url          | string  | 画像取得URL       |
| expires_at   | string  | 有効期限(RFC3339) |
| width        | integer | 出力画像幅         |
| height       | integer | 出力画像高さ        |
| content_type | string  | 常に image/webp |
| size_bytes   | integer | 保存画像サイズ       |

---

## エラーResponse

形式

{
"error": {
"code": "ERROR_CODE",
"message": "description"
}
}

---

## エラーコード一覧

| Code                   | HTTP | 説明         |
| ---------------------- | ---- | ---------- |
| INVALID_PARAMETER      | 400  | パラメータ不正    |
| MISSING_FILE           | 400  | fileが存在しない |
| FILE_TOO_LARGE         | 413  | ファイルサイズ超過  |
| UNSUPPORTED_MEDIA_TYPE | 415  | 未対応画像形式    |
| INVALID_IMAGE          | 400  | 画像デコード失敗   |
| INTERNAL_ERROR         | 500  | サーバ内部エラー   |

---

# 5. 画像取得

## Endpoint

GET /i/{id}

---

## Path Parameter

| Name | Type   | Description |
| ---- | ------ | ----------- |
| id   | string | 画像識別子       |

---

## 処理手順

サーバは以下の処理を行う。

1. SQLiteから画像メタデータを取得する
2. レコードが存在しない場合 404
3. deleted_at が NULL でない場合 404
4. expires_at を確認する
5. 期限切れの場合 404
6. R2から画像を取得する
7. 画像データを返す

---

## Response

Status

200 OK

Headers

Content-Type: image/webp
Cache-Control: no-store
X-Content-Type-Options: nosniff

Body

画像バイナリ

---

## エラー

404 Not Found

以下の場合に返す。

* id が存在しない
* 画像が削除済み
* 画像が期限切れ

---

# 6. ヘルスチェック

## Endpoint

GET /healthz

---

## Purpose

サーバが起動しているか確認するためのエンドポイント。

---

## Response

200 OK

{
"status": "ok"
}

---

# 7. HTTPヘッダ方針

画像レスポンスには以下ヘッダを付与する。

Content-Type: image/webp
Cache-Control: no-store
X-Content-Type-Options: nosniff

---

# 8. 時刻フォーマット

すべての時刻は **RFC3339 UTC** を使用する。

例

2026-03-06T15:00:00Z

---

# 9. ID仕様

画像IDは以下の条件を満たす必要がある。

* 一意である
* 推測困難
* URL安全

推奨形式:

ULID

例:

01JXYZABCDEF1234567890ABCD