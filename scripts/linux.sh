if command -v rpm2 &>/dev/null; then
  echo "Updating rpm2..."
else
  echo "Installing rpm2..."
fi && \
curl -fsSL https://github.com/zevlion/rpm2/releases/download/latest/rpm2 -o /tmp/rpm2_bin && \
chmod +x /tmp/rpm2_bin && \
sudo mv /tmp/rpm2_bin /usr/local/bin/rpm2 && \
echo "Done! $(rpm2 --version 2>/dev/null || true)"