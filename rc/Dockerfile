FROM python:3.13-slim

RUN pip install --no-cache-dir \
      tree-sitter==0.25.2 \
      tree-sitter-rust==0.24.1 \
      click==8.2.1 \
      scrapling==0.4.2 \
      certifi

COPY rc/ /opt/rc/
WORKDIR /opt/rc

EXPOSE 7333

CMD ["python3", "rc_server.py"]
