import { PrismaClient } from "@prisma/client";
import { testGame01,testGame01CreateArgs } from "./seed/testGame01";
import { testGame02 } from "./seed/testGame02";

const prisma = new PrismaClient();

async function main() {
  // clean the database
  await prisma.game.deleteMany();
  await prisma.activeGame.deleteMany();
  await prisma.completedGame.deleteMany();
  await prisma.gameMember.deleteMany();
  await prisma.gameStats.deleteMany();
  await prisma.gameMember.deleteMany();
  await prisma.memberScore.deleteMany();
  await prisma.question.deleteMany();

  // there should be an existing user (from auth0)
  // todo: figure out a way to automate create/delete of test auth0 user
  const testUser = await prisma.user.findUnique({
    where: { email: "colanzio5@gmail.com" },
    select: { id: true, email: true },
  });
  if (!testUser?.id) throw Error("test user not found");

  // create completed game
  const game01 = await prisma.game.create(testGame01CreateArgs)
  const gameMember = await prisma.gameMember.create({
    data: {
      isOwner: true,
      user: { connect: { email: "colanzio5@gmail.com" } },
    }
  })
  await prisma.completedGame.create({
    data: {
      game: {
        connect: { id: game01.id }
      },
      gameMembers: {
        connect: { id: gameMember.id }
      },
      gameStats: {
        create: {
          memberScores: {
            create: {
              score: 10,
              member: { 
                create: {
                  isOwner: true,
                  user: {
                    connect: { id: testUser.id }
                  }
                } 
              }
            }
          }
        }
      },
    },
  });

  // create actove game
  await prisma.activeGame.create({
    data: {
      gameMembers: {
        create: {
          isOwner: true,
          user: {
            connect: { email: "colanzio5@gmail.com" },
          },
        },
      },
      game: {
        create: testGame02,
      },
    },
  });
}

main()
  .then(async () => {
    await prisma.$disconnect();
  })
  .catch(async (e) => {
    console.error(e);
    await prisma.$disconnect();
    process.exit(1);
  });
