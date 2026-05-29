import { EventEmitter } from "node:events";
import type { IncomingMessage } from "node:http";
import { applyWSSHandler } from "@trpc/server/adapters/ws";
import type { Message, Peer } from "crossws";
import type { WebSocket, WebSocketServer } from "ws";
import { activeWebsockets } from "~/lib/metrics";
import { createContext } from "~/server/trpc/context";
import { appRouter } from "~/server/trpc/router";

class TrpcPeerSocket extends EventEmitter {
  readonly OPEN = 1;
  readyState = this.OPEN;

  constructor(private readonly peer: Peer) {
    super();
  }

  send(data: string) {
    this.peer.send(data);
  }

  close() {
    this.readyState = 3;
    this.peer.close();
  }

  emitMessage(message: Message) {
    this.emit("message", message.text());
  }

  emitClose() {
    this.readyState = 3;
    this.emit("close");
  }
}

const clients = new Set<TrpcPeerSocket>();
const wss = Object.assign(new EventEmitter(), {
  clients,
  close() {
    for (const client of clients) {
      client.close();
    }
  },
}) as unknown as WebSocketServer;

applyWSSHandler({
  wss,
  router: appRouter,
  createContext: ({ req }) => createContext(req),
});

function getSocket(peer: Peer): TrpcPeerSocket | undefined {
  return peer.context.trpcSocket as TrpcPeerSocket | undefined;
}

export default defineWebSocketHandler({
  open(peer) {
    const socket = new TrpcPeerSocket(peer);
    peer.context.trpcSocket = socket;
    clients.add(socket);
    activeWebsockets.set(clients.size);
    wss.emit("connection", socket as unknown as WebSocket, peer.request as unknown as IncomingMessage);
  },
  message(peer, message) {
    getSocket(peer)?.emitMessage(message);
  },
  close(peer) {
    const socket = getSocket(peer);
    if (!socket) {
      return;
    }
    socket.emitClose();
    clients.delete(socket);
    delete peer.context.trpcSocket;
    activeWebsockets.set(clients.size);
  },
  error(peer, error) {
    getSocket(peer)?.emit("error", error);
  },
});
