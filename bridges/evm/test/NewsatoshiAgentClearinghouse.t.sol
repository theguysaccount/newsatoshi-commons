// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "../NewsatoshiAgentClearinghouse.sol";

interface VmClearing {
    function warp(uint256) external;
    function prank(address) external;
    function expectRevert(bytes calldata) external;
}

contract MockUsdcClearing {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    function mint(address to, uint256 amt) external {
        balanceOf[to] += amt;
    }

    function approve(address spender, uint256 amt) external returns (bool) {
        allowance[msg.sender][spender] = amt;
        return true;
    }

    function transfer(address to, uint256 val) external returns (bool) {
        require(balanceOf[msg.sender] >= val, "BAL");
        balanceOf[msg.sender] -= val;
        balanceOf[to] += val;
        return true;
    }

    function transferFrom(address from, address to, uint256 val) external returns (bool) {
        require(balanceOf[from] >= val, "BAL");
        require(allowance[from][msg.sender] >= val, "ALLOW");
        allowance[from][msg.sender] -= val;
        balanceOf[from] -= val;
        balanceOf[to] += val;
        return true;
    }
}

contract NewsatoshiAgentClearinghouseTest {
    VmClearing constant vm = VmClearing(address(uint160(uint256(keccak256("hevm cheat code")))));

    NewsatoshiAgentClearinghouse clearing;
    MockUsdcClearing usdc;

    address client = address(0xC11E47);
    address worker = address(0x0000BEEF);
    address subA = address(0xA001);
    address subB = address(0xA002);
    address subC = address(0xA003);

    bytes constant PROOF_PAYLOAD = "lean4:zero-sorry-audit-certificate-v1";
    bytes32 digest;

    function setUp() public {
        clearing = new NewsatoshiAgentClearinghouse();
        usdc = new MockUsdcClearing();
        digest = sha256(PROOF_PAYLOAD);
        vm.warp(1_800_000_000);
        usdc.mint(client, 1_000_000e6);
        usdc.mint(worker, 500_000e6);
    }

    function testSlaCreationVerifiedDagSplitAndStakeRefund() public {
        vm.prank(client);
        usdc.approve(address(clearing), 100e6);
        vm.prank(worker);
        usdc.approve(address(clearing), 10e6);

        vm.prank(client);
        bytes32 slaId = clearing.createSla(client, worker, address(usdc), 100e6, 10e6, digest, uint64(block.timestamp + 3600), 1);

        SwarmNode[] memory nodes = new SwarmNode[](3);
        nodes[0] = SwarmNode({payout: subA, shareBps: 3000});
        nodes[1] = SwarmNode({payout: subB, shareBps: 4000});
        nodes[2] = SwarmNode({payout: subC, shareBps: 2000});

        // Forged payload rejected
        vm.prank(worker);
        vm.expectRevert(bytes("PREDICATE_MISMATCH"));
        clearing.settleAndSplitDag(slaId, "hallucinated-output", nodes);

        uint256 workerBefore = usdc.balanceOf(worker);
        vm.prank(worker);
        clearing.settleAndSplitDag(slaId, PROOF_PAYLOAD, nodes);

        require(usdc.balanceOf(subA) == 30e6, "SUB_A_BAD");
        require(usdc.balanceOf(subB) == 40e6, "SUB_B_BAD");
        require(usdc.balanceOf(subC) == 20e6, "SUB_C_BAD");
        // Worker gets 10e6 coordinator remainder + 10e6 refunded stake = 20e6
        require(usdc.balanceOf(worker) == workerBefore + 20e6, "WORKER_BAD");
        require(usdc.balanceOf(address(clearing)) == 0, "DUST_LEFT");

        (uint64 completed, uint64 slashed, uint256 vol, ) = clearing.reputations(worker);
        require(completed == 1 && slashed == 0 && vol == 100e6, "REP_BAD");
    }

    function testExpiredFraudSlash5050CompensatesClientAndBurns() public {
        vm.prank(client);
        usdc.approve(address(clearing), 100e6);
        vm.prank(worker);
        usdc.approve(address(clearing), 11e6);

        uint64 dl = uint64(block.timestamp + 3600);
        vm.prank(client);
        bytes32 slaId = clearing.createSla(client, worker, address(usdc), 100e6, 11e6, digest, dl, 2);

        vm.expectRevert(bytes("NOT_EXPIRED"));
        clearing.slashExpired(slaId);

        vm.warp(dl + 1);
        uint256 clientBefore = usdc.balanceOf(client);
        clearing.slashExpired(slaId);

        // Client gets 100e6 refunded budget + 5.5e6 (50% of 11e6) compensation; 5.5e6 burned permanently
        require(usdc.balanceOf(client) == clientBefore + 105_500_000, "CLIENT_COMP_BAD");
        require(usdc.balanceOf(clearing.BURN_ADDRESS()) == 5_500_000, "BURN_BAD");
        require(usdc.balanceOf(address(clearing)) == 0, "DUST_LEFT");

        (, uint64 slashed, , uint256 slashedAmt) = clearing.reputations(worker);
        require(slashed == 1 && slashedAmt == 11e6, "SLASH_REP_BAD");
    }

    function testRejectsUnderStakedWorker() public {
        vm.prank(client);
        usdc.approve(address(clearing), 100e6);
        vm.prank(worker);
        usdc.approve(address(clearing), 1e6);

        vm.prank(client);
        vm.expectRevert(bytes("STAKE_BELOW_5PCT"));
        clearing.createSla(client, worker, address(usdc), 100e6, 1e6, digest, uint64(block.timestamp + 3600), 3);
    }

    function testFuzzDagSplitZeroDustConservation(uint96 rawBudget, uint16 rawA, uint16 rawB) public {
        uint256 budget = (uint256(rawBudget) % 500_000e6) + 1000;
        uint256 stake = (budget * 500) / 10_000 + 1;
        uint16 bpsA = uint16((uint256(rawA) % 4999) + 1);
        uint16 bpsB = uint16((uint256(rawB) % 4999) + 1);

        vm.prank(client);
        usdc.approve(address(clearing), budget);
        vm.prank(worker);
        usdc.approve(address(clearing), stake);

        vm.prank(client);
        bytes32 slaId = clearing.createSla(client, worker, address(usdc), budget, stake, digest, uint64(block.timestamp + 3600), uint256(bpsA) * 100_000 + bpsB);

        SwarmNode[] memory nodes = new SwarmNode[](2);
        nodes[0] = SwarmNode({payout: subA, shareBps: bpsA});
        nodes[1] = SwarmNode({payout: subB, shareBps: bpsB});

        uint256 a0 = usdc.balanceOf(subA);
        uint256 b0 = usdc.balanceOf(subB);
        uint256 w0 = usdc.balanceOf(worker);

        vm.prank(worker);
        clearing.settleAndSplitDag(slaId, PROOF_PAYLOAD, nodes);

        uint256 totalDelta = (usdc.balanceOf(subA) - a0) + (usdc.balanceOf(subB) - b0) + (usdc.balanceOf(worker) - w0);
        require(totalDelta == budget + stake, "FUZZ_CONSERVATION_VIOLATED");
        require(usdc.balanceOf(address(clearing)) == 0, "FUZZ_DUST_LEAK");
    }
}
