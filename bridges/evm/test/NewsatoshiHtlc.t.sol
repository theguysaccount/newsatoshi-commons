// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "../NewsatoshiHtlc.sol";

interface Vm {
    function warp(uint256) external;
    function prank(address) external;
    function deal(address, uint256) external;
    function expectRevert(bytes calldata) external;
}

contract MockERC20 {
    string public name = "Mock USDC";
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;
    uint256 public feeBps;

    constructor(uint256 _feeBps) {
        feeBps = _feeBps;
    }

    function mint(address to, uint256 amount) external {
        balanceOf[to] += amount;
    }

    function approve(address spender, uint256 amount) external returns (bool) {
        allowance[msg.sender][spender] = amount;
        return true;
    }

    function transfer(address to, uint256 value) external returns (bool) {
        require(balanceOf[msg.sender] >= value, "BAL");
        balanceOf[msg.sender] -= value;
        uint256 fee = (value * feeBps) / 10000;
        balanceOf[to] += value - fee;
        return true;
    }

    function transferFrom(address from, address to, uint256 value) external returns (bool) {
        require(balanceOf[from] >= value, "BAL");
        require(allowance[from][msg.sender] >= value, "ALLOW");
        allowance[from][msg.sender] -= value;
        balanceOf[from] -= value;
        uint256 fee = (value * feeBps) / 10000;
        balanceOf[to] += value - fee;
        return true;
    }
}

contract NewsatoshiHtlcTest {
    Vm constant vm = Vm(address(uint160(uint256(keccak256("hevm cheat code")))));

    NewsatoshiHtlc htlc;
    MockERC20 usdc;
    MockERC20 feeToken;

    address alice = address(0xA11CE);
    address bob = address(0xB0B);

    // Canonical 32-byte preimage matching Newsatoshi L1 & BIP-199 test vectors
    bytes32 constant PREIMAGE = bytes32(uint256(0x1111111111111111111111111111111111111111111111111111111111111111));
    bytes32 paymentHash;

    function setUp() public {
        htlc = new NewsatoshiHtlc();
        usdc = new MockERC20(0);
        feeToken = new MockERC20(100); // 1% fee-on-transfer adversarial token
        paymentHash = sha256(abi.encodePacked(PREIMAGE));
        vm.warp(1_760_000_000);
        vm.deal(alice, 10 ether);
        usdc.mint(alice, 10_000e6);
        feeToken.mint(alice, 10_000e6);
    }

    function testEthLockAndClaimWithSha256Preimage() public {
        uint64 tl = uint64(block.timestamp + 3600);
        vm.prank(alice);
        bytes32 swapId = htlc.lockEth{value: 1 ether}(bob, paymentHash, tl);

        uint256 bobBefore = bob.balance;
        // Anyone can relay the preimage; funds always settle strictly to recipient (bob)
        htlc.claim(swapId, PREIMAGE);

        require(bob.balance == bobBefore + 1 ether, "BOB_NOT_PAID");
        (, , , , , , bool claimed, bool refunded, bytes32 revealed) = htlc.swaps(swapId);
        require(claimed && !refunded && revealed == PREIMAGE, "BAD_STATE");
    }

    function testErc20UsdcLockAndClaim() public {
        uint64 tl = uint64(block.timestamp + 7200);
        vm.prank(alice);
        usdc.approve(address(htlc), 500e6);

        vm.prank(alice);
        bytes32 swapId = htlc.lockErc20(bob, address(usdc), 500e6, paymentHash, tl);

        htlc.claim(swapId, PREIMAGE);
        require(usdc.balanceOf(bob) == 500e6, "USDC_NOT_PAID");
    }

    function testRejectsBadPreimage() public {
        uint64 tl = uint64(block.timestamp + 3600);
        vm.prank(alice);
        bytes32 swapId = htlc.lockEth{value: 0.5 ether}(bob, paymentHash, tl);

        vm.expectRevert("BAD_PREIMAGE");
        htlc.claim(swapId, bytes32(uint256(0x2222)));
    }

    function testRefundOnlyAfterTimelock() public {
        uint64 tl = uint64(block.timestamp + 3600);
        vm.prank(alice);
        bytes32 swapId = htlc.lockEth{value: 1 ether}(bob, paymentHash, tl);

        vm.expectRevert("TIMELOCK_ACTIVE");
        htlc.refund(swapId);

        vm.warp(tl);
        uint256 aliceBefore = alice.balance;
        htlc.refund(swapId);
        require(alice.balance == aliceBefore + 1 ether, "ALICE_NOT_REFUNDED");

        // Post-refund claim must revert
        vm.expectRevert("ALREADY_SETTLED");
        htlc.claim(swapId, PREIMAGE);
    }

    function testRejectsClaimAtOrAfterTimelock() public {
        uint64 tl = uint64(block.timestamp + 3600);
        vm.prank(alice);
        bytes32 swapId = htlc.lockEth{value: 1 ether}(bob, paymentHash, tl);

        vm.warp(tl);
        vm.expectRevert("TIMELOCK_EXPIRED");
        htlc.claim(swapId, PREIMAGE);
    }

    function testRejectsFeeOnTransferToken() public {
        uint64 tl = uint64(block.timestamp + 3600);
        vm.prank(alice);
        feeToken.approve(address(htlc), 1000e6);

        vm.prank(alice);
        vm.expectRevert("FEE_ON_TRANSFER_UNSUPPORTED");
        htlc.lockErc20(bob, address(feeToken), 1000e6, paymentHash, tl);
    }

    function testRejectsMalformedTimelocks() public {
        vm.prank(alice);
        vm.expectRevert("TIMELOCK_TOO_SOON");
        htlc.lockEth{value: 1 ether}(bob, paymentHash, uint64(block.timestamp + 60));

        vm.prank(alice);
        vm.expectRevert("TIMELOCK_TOO_FAR");
        htlc.lockEth{value: 1 ether}(bob, paymentHash, uint64(block.timestamp + 31 days));
    }
}
