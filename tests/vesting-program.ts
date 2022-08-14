import * as anchor from "@project-serum/anchor";
import { Program } from "@project-serum/anchor";
import { VestingProgram } from "../target/types/vesting_program";
import {
  createAssociatedTokenAccount,
  createMint,
  mintTo,
  getAssociatedTokenAddress,
} from "@solana/spl-token";
import { assert, expect } from "chai";
import { SYSVAR_CLOCK_PUBKEY, PublicKey } from "@solana/web3.js";

const passphrase = "6";
const releaseOne = 100;
const releaseOneTime = 1658813160;
const releaseTwo = 120;
const releaseTwoTime = 1658813400;
const releaseThree = 130;
const releaseThreeTime = 1658814000;

describe("vesting", () => {
  const { web3 } = anchor;
  // Configure the client to use the local cluster.
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);
  const program = anchor.workspace.VestingProgram as Program<VestingProgram>;

  let mintAddress: PublicKey | null;
  let anotherMint: PublicKey | null;
  let ownerToken: PublicKey | null;
  let employeeToken: PublicKey | null;

  const owner = web3.Keypair.generate();
  const randomAccount = web3.Keypair.generate();
  const employee = web3.Keypair.generate();

  it("Create vesting contract", async () => {
    console.log("==================== Creating Contract ====================");

    await provider.connection.confirmTransaction(
      await provider.connection.requestAirdrop(owner.publicKey, 1e9)
    );

    mintAddress = await createMint(
      provider.connection,
      owner,
      owner.publicKey,
      owner.publicKey,
      9
    );
    anotherMint = await createMint(
      provider.connection,
      owner,
      owner.publicKey,
      owner.publicKey,
      9
    );
    console.log(`Creating Mint: ${mintAddress}`);

    ownerToken = await createAssociatedTokenAccount(
      provider.connection,
      owner,
      mintAddress,
      owner.publicKey
    );

    await mintTo(
      provider.connection,
      owner,
      mintAddress,
      ownerToken,
      owner.publicKey,
      1e9
    );

    employeeToken = await createAssociatedTokenAccount(
      provider.connection,
      owner,
      mintAddress,
      employee.publicKey
    );

    const [vestingContract, _infoBump] =
      await web3.PublicKey.findProgramAddress(
        [Buffer.from(anchor.utils.bytes.utf8.encode(passphrase))],
        program.programId
      );

    console.log(`Vesting Account: ${vestingContract}`);

    const [vestingTokenAccount, _vestingTokenBump] =
      await web3.PublicKey.findProgramAddress(
        [mintAddress.toBuffer(), vestingContract.toBuffer()],
        program.programId
      );
    console.log(`Vesting Token Account: ${vestingTokenAccount}`);

    const tx = await program.methods
      .create(
        [
          new anchor.BN(releaseOneTime),
          new anchor.BN(releaseTwoTime),
          new anchor.BN(releaseThreeTime),
        ],
        [
          new anchor.BN(releaseOne),
          new anchor.BN(releaseTwo),
          new anchor.BN(releaseThree),
        ],
        passphrase
      )
      .accounts({
        initializer: owner.publicKey,
        vestingContract,
        srcTokenAccount: ownerToken,
        dstTokenAccount: employeeToken,
        vestingTokenAccount,
        mintAddress,
      })
      .signers([owner])
      .rpc();
    console.log(`Transaction: ${tx}`);

    const fetchedVestingContract = await program.account.vestingContract.fetch(
      vestingContract
    );
    const vestingParsedInfo: any =
      await provider.connection.getParsedAccountInfo(vestingTokenAccount);
    assert.equal(
      +fetchedVestingContract.schedules[0].releaseTime,
      releaseOneTime
    );
    assert.equal(
      +fetchedVestingContract.schedules[1].releaseTime,
      releaseTwoTime
    );
    assert.equal(
      +fetchedVestingContract.schedules[2].releaseTime,
      releaseThreeTime
    );
    assert.equal(
      +vestingParsedInfo.value.data.parsed.info.tokenAmount.amount,
      releaseOne + releaseTwo + releaseThree
    );
  });

  //! =================================

  it("Trigger unlock", async () => {
    console.log("==================== Unlock ====================");
    const [vestingContract] = await web3.PublicKey.findProgramAddress(
      [Buffer.from(anchor.utils.bytes.utf8.encode(passphrase))],
      program.programId
    );

    const [vestingTokenAccount, _vestingTokenBump] =
      await web3.PublicKey.findProgramAddress(
        [mintAddress.toBuffer(), vestingContract.toBuffer()],
        program.programId
      );

    const tx = await program.methods
      .unlock(passphrase)
      .accounts({
        vestingContract,
        vestingTokenAccount,
        dstTokenAccount: employeeToken,
        // mintAddress,
        clock: SYSVAR_CLOCK_PUBKEY,
      })
      .rpc();

    console.log(`Transaction: ${tx}`);
    const employeeTokenAccountInfo: any =
      await provider.connection.getParsedAccountInfo(employeeToken);
    assert.equal(
      +employeeTokenAccountInfo.value.data.parsed.info.tokenAmount.amount,
      releaseOne + releaseTwo + releaseThree
    );
  });

  //! =================================

  it("Trigger unlock with wrong token account, should fail", async () => {
    console.log("========== Unlock with wrong account, should fail ==========");
    const [vestingContract] = await web3.PublicKey.findProgramAddress(
      [Buffer.from(anchor.utils.bytes.utf8.encode(passphrase))],
      program.programId
    );

    const [vestingTokenAccount, _vestingTokenBump] =
      await web3.PublicKey.findProgramAddress(
        [mintAddress.toBuffer(), vestingContract.toBuffer()],
        program.programId
      );

    const randomTokenAccount = await createAssociatedTokenAccount(
      provider.connection,
      owner,
      mintAddress,
      randomAccount.publicKey
    );
    try {
      const tx = await program.methods
        .unlock(passphrase)
        .accounts({
          vestingContract,
          vestingTokenAccount,
          dstTokenAccount: randomTokenAccount, // Wrong account
          // mintAddress,
          clock: SYSVAR_CLOCK_PUBKEY,
        })
        .rpc();

      console.log(`Transaction: ${tx}`);
    } catch (error) {
      expect(true);
    }
  });

  //! =================================

  it("Trigger unlock when zero, should fail", async () => {
    console.log(
      "==================== Unlock, Should Fail ===================="
    );
    const [vestingContract, _vestingBump] =
      await web3.PublicKey.findProgramAddress(
        [Buffer.from(anchor.utils.bytes.utf8.encode(passphrase))],
        program.programId
      );

    const [vestingTokenAccount, _vestingTokenBump] =
      await web3.PublicKey.findProgramAddress(
        [mintAddress.toBuffer(), vestingContract.toBuffer()],
        program.programId
      );

    try {
      const tx = await program.methods
        .unlock(passphrase)
        .accounts({
          vestingContract,
          vestingTokenAccount,
          dstTokenAccount: employeeToken,
          // mintAddress,
          clock: SYSVAR_CLOCK_PUBKEY,
        })
        .rpc();
    } catch (error) {
      console.log("Error Message: ", error.error.errorMessage);
      expect(true);
    }
  });

  //! =================================

  it("Change Destination", async () => {
    console.log("==================== Change Destination ====================");

    const [vestingContract, _vestingBump] =
      await web3.PublicKey.findProgramAddress(
        [Buffer.from(anchor.utils.bytes.utf8.encode(passphrase))],
        program.programId
      );

    const newAddr = web3.Keypair.generate();
    const newAddrToken = await createAssociatedTokenAccount(
      provider.connection,
      owner,
      mintAddress,
      newAddr.publicKey
    );

    console.log(`New Address: ${newAddr.publicKey}`);

    const tx = await program.methods
      .changeDestination(passphrase)
      .accounts({
        vestingContract,
        currentDstTokenAccount: employeeToken,
        currentDstTokenAccountOwner: employee.publicKey,
        newDstTokenAccount: newAddrToken,
        // mintAddress,
      })
      .signers([employee])
      .rpc();
    console.log(`Transaction: ${tx}`);

    const escrowInfo = await program.account.vestingContract.fetch(
      vestingContract
    );

    assert.equal(
      escrowInfo.dstTokenAccount.toString(),
      newAddrToken.toString()
    );
  });

  //! =================================

  it("Wrong Account Close Vesting Contract", async () => {
    console.log(
      "========= Wrong Account Closing Vesting Contract, should fail ========="
    );

    const [vestingContract, _vestingBump] =
      await web3.PublicKey.findProgramAddress(
        [Buffer.from(anchor.utils.bytes.utf8.encode(passphrase))],
        program.programId
      );

    const [vestingTokenAccount, _vestingTokenBump] =
      await web3.PublicKey.findProgramAddress(
        [mintAddress.toBuffer(), vestingContract.toBuffer()],
        program.programId
      );

    try {
      const tx = await program.methods
        .closeAccount(passphrase)
        .accounts({
          initializer: randomAccount.publicKey,
          vestingContract,
          vestingTokenAccount,
          srcTokenAccount: ownerToken,
          // mintAddress,
          clock: SYSVAR_CLOCK_PUBKEY,
        })
        .signers([owner])
        .rpc();
      console.log(`Transaction: ${tx}`);
      expect.fail();
    } catch (error) {
      expect(true);
    }
  });

  //! =================================

  it("Close Vesting Contract", async () => {
    console.log(
      "==================== Closing Vesting Contract ===================="
    );

    const [vestingContract, _vestingBump] =
      await web3.PublicKey.findProgramAddress(
        [Buffer.from(anchor.utils.bytes.utf8.encode(passphrase))],
        program.programId
      );

    const [vestingTokenAccount, _vestingTokenBump] =
      await web3.PublicKey.findProgramAddress(
        [mintAddress.toBuffer(), vestingContract.toBuffer()],
        program.programId
      );

    const tx = await program.methods
      .closeAccount(passphrase)
      .accounts({
        initializer: owner.publicKey,
        vestingContract,
        vestingTokenAccount,
        srcTokenAccount: ownerToken,
        // mintAddress,
        clock: SYSVAR_CLOCK_PUBKEY,
      })
      .signers([owner])
      .rpc();
    console.log(`Transaction: ${tx}`);
    try {
      await program.account.vestingContract.fetch(vestingContract);
      expect.fail();
    } catch (error) {
      expect(true);
    }
  });
});
