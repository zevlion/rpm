if command -v rpm2 &>/dev/null; then
  echo "Updating rpm2..."
else
  echo "Installing rpm2..."
fi && \
curl -L https://github.com/zevlion/rpm2/releases/download/latest/rpm2 -o rpm2 && \
chmod +x rpm2 && \
sudo mv rpm2 /usr/local/bin/rpm2 && \
echo "Done! rpm2 $(rpm2 --version 2>/dev/null || true)"