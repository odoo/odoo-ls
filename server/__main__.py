import argparse
import logging

from server.controller import odoo_server
from server.constants import *

FORMAT = '%(asctime)s %(levelname)s: %(message)s'

def add_arguments(parser):
    parser.description = "simple odoo server example"

    parser.add_argument(
        "--tcp", action="store_true",
        help="Use TCP server"
    )
    parser.add_argument(
        "--ws", action="store_true",
        help="Use WebSocket server"
    )
    parser.add_argument(
        "--host", default="127.0.0.1",
        help="Bind to this address"
    )
    parser.add_argument(
        "--port", type=int, default=2087,
        help="Bind to this port"
    )
    parser.add_argument(
        "--log", type=str, default="pygls.log",
        help="Debug log file name"
    )
    parser.add_argument(
        "--id", type=str, default="clean-odoo-lsp",
        help="Identifier to help find process"
    )


def main():
    parser = argparse.ArgumentParser()
    add_arguments(parser)
    args = parser.parse_args()
    logging.basicConfig(format=FORMAT, datefmt='%Y-%m-%d %I:%M:%S', filename=args.log, level=logging.DEBUG, filemode="w")

    if "alpha" in EXTENSION_VERSION:
        logging.getLogger().setLevel(logging.DEBUG)

    if args.tcp:
        odoo_server.start_tcp(args.host, args.port)
    elif args.ws:
        odoo_server.start_ws(args.host, args.port)
    else:
        odoo_server.start_io()


if __name__ == '__main__':
    main()
