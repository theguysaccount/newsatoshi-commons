// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "../NewsatoshiErc7683Settler.sol";

interface Vm7683 {
    function warp(uint256) external;
    function prank(address) external;
    function addr(uint256 privateKey) external pure returns (address);
    function sign(uint256 privateKey, bytes32 digest) external pure returns (uint8 v, bytes32 r, bytes32 s);
    function expectRevert(bytes calldata) external;
}

contract MockUsdc7683 {
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;
    uint256 public feeBps;

    constructor(uint256 _feeBps) {
        feeBps = _feeBps;
    }

    function mint(address to, uint256 amt) external {
        balanceOf[to] += amt;
    }

    function approve(address spender, uint256 amt) external returns (bool) {
        allowance[msg.sender][spender] = amt;
        return true;
    }

    function transfer(address to, uint256 value) external returns (bool) {
        require(balanceOf[msg.sender] >= value, "BAL");
        balanceOf[msg.sender] -= value;
        uint256 fee = (value * feeBps) / 10_000;
        balanceOf[to] += value - fee;
        return true;
    }

    function transferFrom(address from, address to, uint256 value) external returns (bool) {
        require(balanceOf[from] >= value, "BAL");
        require(allowance[from][msg.sender] >= value, "ALLOW");
        allowance[from][msg.sender] -= value;
        balanceOf[from] -= value;
        uint256 fee = (value * feeBps) / 10_000;
        balanceOf[to] += value - fee;
        return true;
    }
}

contract NewsatoshiErc7683SettlerTest {
    Vm7683 constant vm = Vm7683(address(uint160(uint256(keccak256("hevm cheat code")))));

    NewsatoshiErc7683Settler settler;
    MockUsdc7683 usdc;
    MockUsdc7683 feeToken;

    uint256 constant ALICE_PK = 0xA11CE00000000000000000000000000000000000000000000000000000000001;
    address alice;
    address solver = address(0x5017E8);

    bytes32 constant PREIMAGE = bytes32(uint256(0x2222222222222222222222222222222222222222222222222222222222222222));
    bytes32 paymentHash;

    function setUp() public {
        settler = new NewsatoshiErc7683Settler();
        usdc = new MockUsdc7683(0);
        feeToken = new MockUsdc7683(100);
        alice = vm.addr(ALICE_PK);
        paymentHash = sha256(abi.encodePacked(PREIMAGE));
        vm.warp(1_800_000_000);
        usdc.mint(alice, 1_000_000e6);
        feeToken.mint(alice, 1_000_000e6);
    }

    function _makeGaslessOrder(address token, uint256 amount, uint256 nonce)
        internal
        view
        returns (GaslessCrossChainOrder memory order, bytes memory sig)
    {
        bytes memory data = abi.encode(
            NewsatoshiOrderData({
                inputToken: token,
                inputAmount: amount,
                paymentHash: paymentHash,
                destinationNs1Hash: sha256("ns1qerc7683recipient0000000000000001")
            })
        );
        order = GaslessCrossChainOrder({
            originSettler: address(settler),
            user: alice,
            nonce: nonce,
            originChainId: block.chainid,
            openDeadline: uint32(block.timestamp + 1800),
            fillDeadline: uint32(block.timestamp + 7200),
            orderDataType: settler.NEWSATOSHI_ORDER_DATA_TYPE(),
            orderData: data
        });
        bytes32 digest = settler.hashGaslessOrder(order);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, digest);
        sig = abi.encodePacked(r, s, v);
    }

    function testGaslessOpenForAndSha256PreimageClaim() public {
        vm.prank(alice);
        usdc.approve(address(settler), 500e6);

        (GaslessCrossChainOrder memory order, bytes memory sig) = _makeGaslessOrder(address(usdc), 500e6, 1);

        vm.prank(solver);
        bytes32 orderId = settler.openFor(order, sig, "");

        require(usdc.balanceOf(address(settler)) == 500e6, "ESCROW_MISMATCH");

        // Wrong preimage must revert
        vm.expectRevert(bytes("INVALID_PREIMAGE"));
        settler.claimWithPreimage(orderId, bytes32(uint256(0xDEAD)));

        // Valid SHA-256 preimage settles strictly to solver
        settler.claimWithPreimage(orderId, PREIMAGE);
        require(usdc.balanceOf(solver) == 500e6, "SOLVER_NOT_PAID");

        // Replay of the same signed order must revert
        vm.prank(solver);
        vm.expectRevert(bytes("NONCE_USED"));
        settler.openFor(order, sig, "");
    }

    function testOnchainOpenAndRefundAfterExpiry() public {
        vm.prank(alice);
        usdc.approve(address(settler), 250e6);

        bytes memory data = abi.encode(
            NewsatoshiOrderData({
                inputToken: address(usdc),
                inputAmount: 250e6,
                paymentHash: paymentHash,
                destinationNs1Hash: sha256("ns1qrefundtest000000000000000000001")
            })
        );
        OnchainCrossChainOrder memory onchainOrder = OnchainCrossChainOrder({
            fillDeadline: uint32(block.timestamp + 3600),
            orderDataType: settler.NEWSATOSHI_ORDER_DATA_TYPE(),
            orderData: data
        });

        vm.prank(alice);
        bytes32 orderId = settler.open(onchainOrder, solver);

        // Refund before fillDeadline must revert
        vm.expectRevert(bytes("NOT_EXPIRED"));
        settler.refundExpired(orderId);

        // After fillDeadline, anyone can trigger 100% refund strictly back to alice
        vm.warp(block.timestamp + 3600);
        uint256 aliceBefore = usdc.balanceOf(alice);
        settler.refundExpired(orderId);
        require(usdc.balanceOf(alice) == aliceBefore + 250e6, "REFUND_FAILED");
    }

    function testFeeOnTransferBalanceDeltaSafe() public {
        vm.prank(alice);
        feeToken.approve(address(settler), 1000e6);

        (GaslessCrossChainOrder memory order, bytes memory sig) = _makeGaslessOrder(address(feeToken), 1000e6, 2);
        vm.prank(solver);
        bytes32 orderId = settler.openFor(order, sig, "");

        (, , , uint256 recorded, , , , , , ) = settler.orders(orderId);
        require(recorded == 990e6, "FEE_DELTA_NOT_TRACKED");

        settler.claimWithPreimage(orderId, PREIMAGE);
        require(feeToken.balanceOf(address(settler)) == 0, "DUST_LEFT");
    }

    function testFuzzGaslessOrderSettlement(uint96 rawAmt, bytes32 fuzzPreimage) public {
        uint256 amt = (uint256(rawAmt) % 500_000e6) + 1;
        bytes32 h = sha256(abi.encodePacked(fuzzPreimage));

        vm.prank(alice);
        usdc.approve(address(settler), amt);

        bytes memory data = abi.encode(
            NewsatoshiOrderData({
                inputToken: address(usdc),
                inputAmount: amt,
                paymentHash: h,
                destinationNs1Hash: sha256("ns1qfuzz000000000000000000000000001")
            })
        );
        GaslessCrossChainOrder memory order = GaslessCrossChainOrder({
            originSettler: address(settler),
            user: alice,
            nonce: uint256(fuzzPreimage),
            originChainId: block.chainid,
            openDeadline: uint32(block.timestamp + 1800),
            fillDeadline: uint32(block.timestamp + 7200),
            orderDataType: settler.NEWSATOSHI_ORDER_DATA_TYPE(),
            orderData: data
        });
        bytes32 digest = settler.hashGaslessOrder(order);
        (uint8 v, bytes32 r, bytes32 s) = vm.sign(ALICE_PK, digest);

        vm.prank(solver);
        bytes32 orderId = settler.openFor(order, abi.encodePacked(r, s, v), "");
        uint256 beforeSolver = usdc.balanceOf(solver);
        settler.claimWithPreimage(orderId, fuzzPreimage);
        require(usdc.balanceOf(solver) == beforeSolver + amt, "FUZZ_CONSERVATION_BROKEN");
    }
}
