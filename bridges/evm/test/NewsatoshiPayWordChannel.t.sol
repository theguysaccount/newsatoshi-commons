// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
pragma solidity ^0.8.24;

import "../NewsatoshiPayWordChannel.sol";

interface VmPW {
    function warp(uint256) external;
    function prank(address) external;
    function startPrank(address) external;
    function stopPrank() external;
    function deal(address, uint256) external;
    function expectRevert(bytes calldata) external;
}

contract MockUsdcPW {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    function mint(address to, uint256 amt) external {
        balanceOf[to] += amt;
    }

    function approve(address sp, uint256 amt) external returns (bool) {
        allowance[msg.sender][sp] = amt;
        return true;
    }

    function transfer(address to, uint256 amt) external returns (bool) {
        require(balanceOf[msg.sender] >= amt, "BAL");
        balanceOf[msg.sender] -= amt;
        balanceOf[to] += amt;
        return true;
    }

    function transferFrom(address from, address to, uint256 amt) external returns (bool) {
        require(allowance[from][msg.sender] >= amt, "ALLOW");
        require(balanceOf[from] >= amt, "BAL");
        allowance[from][msg.sender] -= amt;
        balanceOf[from] -= amt;
        balanceOf[to] += amt;
        return true;
    }
}

contract NewsatoshiPayWordChannelTest {
    VmPW constant vm = VmPW(address(uint160(uint256(keccak256("hevm cheat code")))));
    NewsatoshiPayWordChannel chan;
    MockUsdcPW usdc;

    address alice = address(0xA11CE);
    address bob = address(0xB0B);

    function setUp() public {
        chan = new NewsatoshiPayWordChannel();
        usdc = new MockUsdcPW();
        vm.deal(alice, 10 ether);
        usdc.mint(alice, 100_000_000); // 100 USDC (6 decimals)
    }

    function _chain(bytes32 seed, uint32 n) internal pure returns (bytes32[] memory w) {
        w = new bytes32[](n + 1);
        w[n] = seed;
        for (uint32 i = n; i > 0; i--) {
            w[i - 1] = sha256(abi.encodePacked(w[i]));
        }
    }

    function testUsdcIncrementalClaimAndEarlyRefund() public {
        uint32 maxSteps = 20;
        uint256 unitAmt = 5_000; // $0.005 USDC per call
        bytes32[] memory w = _chain(keccak256("payword-seed-1"), maxSteps);
        uint64 expiry = uint64(block.timestamp + 1 days);

        vm.startPrank(alice);
        usdc.approve(address(chan), unitAmt * maxSteps);
        bytes32 cid = chan.openChannel(bob, address(usdc), w[0], unitAmt, maxSteps, expiry);
        vm.stopPrank();

        require(usdc.balanceOf(address(chan)) == 100_000, "locked mismatch");

        // Server settles first 7 API calls in 1 tx
        chan.claim(cid, 7, w[7]);
        require(usdc.balanceOf(bob) == 35_000, "claim 7 mismatch");

        // Replaying w[7] or older w[5] must revert
        vm.expectRevert("BAD_PAYWORD_PROOF");
        chan.claim(cid, 1, w[7]);

        // Server settles next 5 API calls (steps 8..12) using w[12]
        chan.claim(cid, 5, w[12]);
        require(usdc.balanceOf(bob) == 60_000, "claim 12 mismatch");

        // Trying to overclaim > remaining 8 steps must revert
        vm.expectRevert("OVERCLAIM");
        chan.claim(cid, 9, w[20]);

        // Recipient closes early -> unused 8 steps ($0.04 USDC) immediately refunded to Alice
        vm.prank(bob);
        chan.closeAndRefund(cid);
        require(usdc.balanceOf(alice) == 100_000_000 - 60_000, "alice refund mismatch");
        require(usdc.balanceOf(address(chan)) == 0, "contract must be empty");

        // Exact conservation: claimed + refunded == total deposit
        (uint32 remSteps, uint256 remAmt) = chan.remaining(cid);
        require(remSteps == 0 && remAmt == 0, "must be closed");
    }

    function testEthChannelExpiryRefund() public {
        uint32 maxSteps = 10;
        uint256 unitAmt = 0.001 ether;
        bytes32[] memory w = _chain(keccak256("eth-seed"), maxSteps);
        uint64 expiry = uint64(block.timestamp + 1 hours);

        vm.prank(alice);
        bytes32 cid = chan.openChannel{value: unitAmt * maxSteps}(bob, address(0), w[0], unitAmt, maxSteps, expiry);

        chan.claim(cid, 4, w[4]);
        require(bob.balance == 0.004 ether, "bob eth payout");

        // Cannot refund before expiry
        vm.expectRevert("CHANNEL_ACTIVE");
        chan.refund(cid);

        // Warp past expiry -> unilateral refund of remaining 6 steps to Alice
        vm.warp(expiry + 1);
        uint256 aliceBefore = alice.balance;
        chan.refund(cid);
        require(alice.balance - aliceBefore == 0.006 ether, "alice eth refund");
        require(address(chan).balance == 0, "eth dust left");
    }

    function testLargeBatch1000StepsSingleTx() public {
        uint32 maxSteps = 1000;
        uint256 unitAmt = 100; // $0.0001 per call
        bytes32[] memory w = _chain(keccak256("scale-1000"), maxSteps);
        uint64 expiry = uint64(block.timestamp + 2 days);

        vm.startPrank(alice);
        usdc.approve(address(chan), unitAmt * maxSteps);
        bytes32 cid = chan.openChannel(bob, address(usdc), w[0], unitAmt, maxSteps, expiry);
        vm.stopPrank();

        // Settle all 1,000 API calls in ONE transaction!
        chan.claim(cid, 1000, w[1000]);
        require(usdc.balanceOf(bob) == 100_000, "1000-step claim mismatch");
        require(usdc.balanceOf(address(chan)) == 0, "channel must be fully drained");
    }
}
