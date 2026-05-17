FROM python:3.11-slim
WORKDIR /app
COPY pyproject.toml requirements.lock ./
RUN pip install --no-cache-dir -r requirements.lock
COPY src/ src/
ENV PYTHONUNBUFFERED=1
CMD ["python", "-m", "minitia"]
