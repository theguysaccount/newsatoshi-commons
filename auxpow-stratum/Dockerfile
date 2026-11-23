FROM python:3.12-slim

WORKDIR /app
COPY stratum_auxpow.py /app/stratum_auxpow.py
RUN chmod +x /app/stratum_auxpow.py

EXPOSE 3333 3334

ENTRYPOINT ["python3", "/app/stratum_auxpow.py"]
