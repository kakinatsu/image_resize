(() => {
  const ACCEPTED_TYPES = new Set(["image/jpeg", "image/png", "image/webp"]);
  const ACCEPTED_EXTENSIONS = [".jpg", ".jpeg", ".png", ".webp"];
  const MIN_DIMENSION = 1;
  const MAX_DIMENSION = 4096;

  const ERROR_MESSAGE_BY_CODE = {
    INVALID_PARAMETER: "入力内容に誤りがあります。最大幅・最大高さを確認してください。",
    MISSING_FILE: "画像ファイルを選択してください。",
    UNSUPPORTED_MEDIA_TYPE: "対応していない画像形式です。JPEG / PNG / WebP を選択してください。",
    INVALID_IMAGE: "画像の読み込みに失敗しました。別の画像でお試しください。",
    INTERNAL_ERROR: "アップロードに失敗しました。時間をおいて再度お試しください。",
  };

  const { createApp } = Vue;

  createApp({
    data() {
      return {
        selectedFile: null,
        localPreviewUrl: "",
        maxWidthInput: "2048",
        maxHeightInput: "2048",
        isDragging: false,
        isUploading: false,
        uploadResult: null,
        maxFileBytes: null,
        errorMessage: "",
        errorCode: "",
        copySuccess: false,
        copyResetTimer: null,
      };
    },
    computed: {
      canUpload() {
        return Boolean(this.selectedFile) && !this.isUploading;
      },
      uiStateLabel() {
        if (this.isUploading) {
          return "アップロード中";
        }

        if (this.uploadResult) {
          return "成功";
        }

        if (this.errorMessage) {
          return "失敗";
        }

        if (this.selectedFile) {
          return "選択済み";
        }

        return "未選択";
      },
      uploadLimitText() {
        if (!this.maxFileBytes) {
          return "画像サイズ上限はサーバーの環境設定に従います。";
        }

        return `画像サイズ上限は ${this.formatUploadLimit(this.maxFileBytes)} です。サーバーの環境設定で変更できます。`;
      },
    },
    methods: {
      async loadSettings() {
        try {
          const response = await fetch("/api/settings", {
            headers: {
              Accept: "application/json",
            },
          });

          if (!response.ok) {
            return;
          }

          const payload = await response.json().catch(() => null);
          const maxFileBytes = payload?.upload?.max_file_bytes;
          if (Number.isFinite(maxFileBytes) && maxFileBytes > 0) {
            this.maxFileBytes = maxFileBytes;
          }
        } catch {
          this.maxFileBytes = null;
        }
      },
      openFilePicker() {
        if (this.isUploading) {
          return;
        }

        this.$refs.fileInput?.click();
      },
      onDragOver() {
        if (this.isUploading) {
          return;
        }

        this.isDragging = true;
      },
      onDragLeave() {
        this.isDragging = false;
      },
      onDrop(event) {
        this.isDragging = false;
        if (this.isUploading) {
          return;
        }

        const file = event.dataTransfer?.files?.[0];
        this.applyFile(file || null);
      },
      handleFileInput(event) {
        const file = event.target?.files?.[0] || null;
        this.applyFile(file);
        event.target.value = "";
      },
      applyFile(file) {
        this.clearError();
        this.clearCopyState();
        this.uploadResult = null;

        if (!file) {
          this.selectedFile = null;
          this.revokePreviewUrl();
          return;
        }

        if (!this.isSupportedFile(file)) {
          this.selectedFile = null;
          this.revokePreviewUrl();
          this.setError(
            "対応していない画像形式です。JPEG / PNG / WebP を選択してください。",
            "UNSUPPORTED_MEDIA_TYPE"
          );
          return;
        }

        if (this.maxFileBytes && file.size > this.maxFileBytes) {
          this.selectedFile = null;
          this.revokePreviewUrl();
          this.setError(this.fileTooLargeMessage(), "FILE_TOO_LARGE");
          return;
        }

        this.selectedFile = file;
        this.revokePreviewUrl();
        this.localPreviewUrl = URL.createObjectURL(file);
      },
      isSupportedFile(file) {
        if (ACCEPTED_TYPES.has(file.type)) {
          return true;
        }

        const lowered = (file.name || "").toLowerCase();
        return ACCEPTED_EXTENSIONS.some((extension) => lowered.endsWith(extension));
      },
      validateDimensions() {
        const maxWidth = this.parseDimension(this.maxWidthInput, "最大幅");
        const maxHeight = this.parseDimension(this.maxHeightInput, "最大高さ");

        return { maxWidth, maxHeight };
      },
      parseDimension(value, fieldLabel) {
        if (value === "" || value === null || value === undefined) {
          throw new Error(`${fieldLabel}を入力してください。`);
        }

        const parsed = Number(value);
        const isInRange = Number.isInteger(parsed) && parsed >= MIN_DIMENSION && parsed <= MAX_DIMENSION;
        if (!isInRange) {
          throw new Error(`${fieldLabel}は1から4096の整数で入力してください。`);
        }

        return parsed;
      },
      async uploadImage() {
        if (!this.selectedFile || this.isUploading) {
          return;
        }

        this.clearError();

        let dimensions;
        try {
          dimensions = this.validateDimensions();
        } catch (error) {
          this.setError(error.message, "INVALID_PARAMETER");
          return;
        }

        const formData = new FormData();
        formData.append("file", this.selectedFile);

        const query = new URLSearchParams({
          max_width: String(dimensions.maxWidth),
          max_height: String(dimensions.maxHeight),
        });

        this.isUploading = true;
        this.uploadResult = null;
        this.clearCopyState();

        try {
          const response = await fetch(`/api/images?${query.toString()}`, {
            method: "POST",
            body: formData,
          });

          const payload = await response.json().catch(() => null);

          if (!response.ok) {
            const { message, code } = this.resolveApiError(payload, response.status);
            this.setError(message, code);
            return;
          }

          this.uploadResult = payload;
        } catch {
          this.setError(
            "アップロードに失敗しました。時間をおいて再度お試しください。",
            "NETWORK_ERROR"
          );
        } finally {
          this.isUploading = false;
        }
      },
      resolveApiError(payload, statusCode) {
        const code = payload?.error?.code;
        if (code === "FILE_TOO_LARGE" || statusCode === 413) {
          return {
            message: this.fileTooLargeMessage(),
            code: code || "FILE_TOO_LARGE",
          };
        }

        if (code && ERROR_MESSAGE_BY_CODE[code]) {
          return {
            message: ERROR_MESSAGE_BY_CODE[code],
            code,
          };
        }

        if (statusCode === 415) {
          return {
            message: "対応していない画像形式です。JPEG / PNG / WebP を選択してください。",
            code: code || "UNSUPPORTED_MEDIA_TYPE",
          };
        }

        if (statusCode === 400) {
          return {
            message: "入力内容に誤りがあります。内容を確認して再度お試しください。",
            code: code || "INVALID_PARAMETER",
          };
        }

        return {
          message: "アップロードに失敗しました。時間をおいて再度お試しください。",
          code: code || "INTERNAL_ERROR",
        };
      },
      async copyUrl() {
        if (!this.uploadResult?.url) {
          return;
        }

        try {
          if (navigator.clipboard?.writeText) {
            await navigator.clipboard.writeText(this.uploadResult.url);
          } else {
            this.fallbackCopy(this.uploadResult.url);
          }

          this.copySuccess = true;
          this.resetCopyTimer();
        } catch {
          this.setError("URLのコピーに失敗しました。手動でコピーしてください。", "COPY_FAILED");
        }
      },
      fallbackCopy(value) {
        const textarea = document.createElement("textarea");
        textarea.value = value;
        textarea.setAttribute("readonly", "readonly");
        textarea.style.position = "fixed";
        textarea.style.left = "-9999px";
        document.body.appendChild(textarea);
        textarea.select();
        const copied = document.execCommand("copy");
        document.body.removeChild(textarea);

        if (!copied) {
          throw new Error("copy failed");
        }
      },
      clearError() {
        this.errorMessage = "";
        this.errorCode = "";
      },
      setError(message, code) {
        this.errorMessage = message;
        this.errorCode = code || "";
      },
      clearCopyState() {
        this.copySuccess = false;

        if (this.copyResetTimer) {
          clearTimeout(this.copyResetTimer);
          this.copyResetTimer = null;
        }
      },
      resetCopyTimer() {
        this.clearCopyState();
        this.copySuccess = true;
        this.copyResetTimer = setTimeout(() => {
          this.copySuccess = false;
          this.copyResetTimer = null;
        }, 1600);
      },
      fileTooLargeMessage() {
        if (!this.maxFileBytes) {
          return "ファイルサイズが設定上限を超えています。";
        }

        return `ファイルサイズが上限（${this.formatUploadLimit(this.maxFileBytes)}）を超えています。`;
      },
      revokePreviewUrl() {
        if (this.localPreviewUrl) {
          URL.revokeObjectURL(this.localPreviewUrl);
          this.localPreviewUrl = "";
        }
      },
      formatBytes(bytes) {
        if (!Number.isFinite(bytes) || bytes < 0) {
          return "不明";
        }

        if (bytes < 1024) {
          return `${bytes} B`;
        }

        if (bytes < 1024 * 1024) {
          return `${(bytes / 1024).toFixed(1)} KB`;
        }

        return `${(bytes / (1024 * 1024)).toFixed(2)} MB`;
      },
      formatUploadLimit(bytes) {
        if (!Number.isFinite(bytes) || bytes <= 0) {
          return "不明";
        }

        if (bytes >= 1000 * 1000) {
          return `${this.formatUploadLimitNumber(bytes / (1000 * 1000))}MB`;
        }

        if (bytes >= 1000) {
          return `${this.formatUploadLimitNumber(bytes / 1000)}KB`;
        }

        return `${bytes}B`;
      },
      formatUploadLimitNumber(value) {
        const digits = value >= 10 ? 0 : 1;
        return value.toFixed(digits).replace(/\.0$/, "");
      },
      formatExpires(value) {
        const date = new Date(value);
        if (Number.isNaN(date.getTime())) {
          return value;
        }

        return date.toLocaleString("ja-JP", {
          year: "numeric",
          month: "2-digit",
          day: "2-digit",
          hour: "2-digit",
          minute: "2-digit",
          second: "2-digit",
          timeZoneName: "short",
          hour12: false,
        });
      },
    },
    mounted() {
      this.loadSettings();
    },
    beforeUnmount() {
      this.revokePreviewUrl();

      if (this.copyResetTimer) {
        clearTimeout(this.copyResetTimer);
      }
    },
  }).mount("#app");
})();
