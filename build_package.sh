PACKAGE_VERSION=$(cat package.json \
  | grep version \
  | head -1 \
  | awk -F: '{ print $2 }' \
  | sed 's/[",]//g')
echo $PACKAGE_VERSION
./node_modules/@vscode/vsce/vsce package -o odoo_$PACKAGE_VERSION
