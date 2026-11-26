// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "../NewsatoshiBaseMirror.sol";
import "../NewsatoshiHtlc.sol";
import "../NewsatoshiBackingVault.sol";

interface Vm {
    function prank(address) external;
    function warp(uint256) external;
}

contract NewsatoshiBaseMirrorTest {
    Vm private constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    NewsatoshiBaseMirror private token;
    NewsatoshiHtlc private htlc;
    NewsatoshiBackingVault private vault;

    address private constant ALICE = address(0xA11CE);
    address private constant BOB = address(0xB0B);

    function setUp() public {
        token = new NewsatoshiBaseMirror();
        htlc = new NewsatoshiHtlc();
        vault = new NewsatoshiBackingVault();
        token.transfer(ALICE, 1_000_000 * 1e18);
    }

    function testMetadataAndFixedSupply() public view {
        require(token.totalSupply() == 21_000_000 * 1e18, "supply");
        require(token.decimals() == 18, "decimals");
        require(token.CANONICAL_HTLC() == 0xF564A8F600B02981Bc02aC62a2B7d0C8dC638987, "htlc");
        require(token.CANONICAL_BACKING_VAULT() == 0x37Ed94525DBE408fA72cDd156409875666679265, "vault");
        require(token.BASE_ERC8004_AGENT_ID() == 1229992, "agentId");
    }

    function testAtomicHtlcLockAndClaimWithWLove() public {
        bytes32 preimage = keccak256("newsatoshi-wlove-atomic-swap-test");
        bytes32 hashLock = sha256(abi.encodePacked(preimage));

        vm.prank(ALICE);
        token.approve(address(htlc), 500 * 1e18);

        vm.prank(ALICE);
        bytes32 swapId = htlc.lockErc20(BOB, address(token), 500 * 1e18, hashLock, uint64(block.timestamp + 3600));

        vm.prank(BOB);
        htlc.claim(swapId, preimage);

        require(token.balanceOf(BOB) == 500 * 1e18, "bob claimed wLOVE");
    }

    function testPermanentDepositIntoBackingVault() public {
        bytes32 charterId = keccak256("charter-test-wlove");
        vm.prank(ALICE);
        token.approve(address(vault), 250 * 1e18);

        vm.prank(ALICE);
        vault.depositErc20(charterId, address(token), 250 * 1e18, keccak256("test backing"));

        (uint256 bal, uint256 deposits,) = vault.getReserveSummary(address(token));
        require(bal == 250 * 1e18 && deposits == 1, "vault reserve");
    }
}
