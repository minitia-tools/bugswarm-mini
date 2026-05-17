FROM python:3.11-slim
WORKDIR /app
COPY pyproject.toml requirements.lock ./
RUN pip install --no-cache-dir -r requirements.lock
COPY src/ src/
ENV PYTHONUNBUFFERED=1
EXPOSE 8080
HEALTHCHECK --interval=15s --timeout=5s CMD python -c "print('ok')" || exit 1
CMD ["python", "-m", "gateway"]
