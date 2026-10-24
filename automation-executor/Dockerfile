FROM python:3.12-slim-bookworm

ARG PLATFORM_TOOLS_VERSION=35.0.2
ARG SCRCPY_VERSION=5.0.1
ARG SCRCPY_SHA256=764eb6f79811d5211fe9df341120882ba9994c7a61b897d7bf3fb662e53bc536
# Google's Linux platform-tools archive is x86_64. ARM needs a separately verified ADB package.
RUN test "$(uname -m)" = "x86_64" \
    && apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl unzip libglib2.0-0 libgl1 \
    && rm -rf /var/lib/apt/lists/* \
    && curl --fail --location --retry 3 "https://dl.google.com/android/repository/platform-tools_r${PLATFORM_TOOLS_VERSION}-linux.zip" -o /tmp/platform-tools.zip \
    && unzip -q /tmp/platform-tools.zip -d /opt \
    && rm /tmp/platform-tools.zip \
    && mkdir -p /opt/automation \
    && curl --fail --location --retry 3 "https://github.com/Genymobile/scrcpy/releases/download/v${SCRCPY_VERSION}/scrcpy-server-v${SCRCPY_VERSION}" -o "/opt/automation/scrcpy-server-v${SCRCPY_VERSION}" \
    && echo "${SCRCPY_SHA256}  /opt/automation/scrcpy-server-v${SCRCPY_VERSION}" | sha256sum -c - \
    && curl --fail --location "https://raw.githubusercontent.com/Genymobile/scrcpy/v${SCRCPY_VERSION}/LICENSE" -o /opt/automation/SCRCPY-LICENSE

ENV PATH="/opt/platform-tools:${PATH}" \
    PYTHONDONTWRITEBYTECODE=1 \
    PYTHONUNBUFFERED=1 \
    AUTOMATION_DATA_DIR=/var/lib/easytier-automation \
    AUTOMATION_SCRCPY_SERVER=/opt/automation/scrcpy-server-v5.0.1
WORKDIR /app
COPY requirements.txt ./requirements.txt
RUN pip install --no-cache-dir --only-binary=av -r requirements.txt \
    && useradd --create-home --uid 10001 automation \
    && mkdir -p /var/lib/easytier-automation /home/automation/.android \
    && chown -R automation:automation /var/lib/easytier-automation /home/automation/.android
COPY automation_executor ./automation_executor
USER automation
CMD ["uvicorn", "automation_executor.app:app", "--host", "127.0.0.1", "--port", "8766", "--workers", "1", "--no-access-log"]
