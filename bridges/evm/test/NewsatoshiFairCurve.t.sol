// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
pragma solidity ^0.8.24;

import "../NewsatoshiFairCurveFactory.sol";

interface VmFC {
    function startPrank(address) external;
    function stopPrank() external;
    function prank(address) external;
    function expectRevert(bytes calldata) external;
}

contract MockUsdcFC {
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

contract NewsatoshiFairCurveTest {
    VmFC constant vm = VmFC(address(uint160(uint256(keccak256("hevm cheat code")))));
    NewsatoshiFairCurveFactory factory;
    MockUsdcFC usdc;

    address alice = address(0xA11CE);
    address bob = address(0xB0B);
    address apiServer = address(0x5E47E4);

    function setUp() public {
        factory = new NewsatoshiFairCurveFactory();
        usdc = new MockUsdcFC();
        usdc.mint(alice, 50_000_000_000); // $50,000 USDC
        usdc.mint(bob, 50_000_000_000);
        usdc.mint(apiServer, 10_000_000_000);
    }

    function testFairCurveSolvencyAndDilutionProofPayWordRevenue() public {
        (address poolAddr, address tokAddr) = factory.createCurve(
            "Agent API Pool",
            "AGAPI",
            address(usdc),
            10_000 * 1e6, // $10,000 virtual USDC depth
            1_000_000_000 * 1e18, // 1B virtual supply
            100, // 1% swap fee routed to holders (feeSink = 0)
            address(0),
            "https://commons.newsatoshi.org"
        );
        FairCurvePool pool = FairCurvePool(poolAddr);
        FairCurveToken tok = FairCurveToken(tokAddr);

        require(tok.totalSupply() == 0, "must have 0 premine");

        // 1. Alice buys $1,000 USDC
        vm.startPrank(alice);
        usdc.approve(poolAddr, 1_000 * 1e6);
        uint256 aliceTok = pool.buy(1_000 * 1e6, 1);
        vm.stopPrank();
        require(aliceTok > 0 && tok.balanceOf(alice) == aliceTok, "alice buy");

        // 2. API server settles $500 USDC of O(1) PayWords earnings directly into the curve!
        vm.startPrank(apiServer);
        usdc.approve(poolAddr, 500 * 1e6);
        pool.injectRevenue(500 * 1e6);
        vm.stopPrank();

        // Alice owns 100% of circulating tokens -> earned ~$500 USDC
        uint256 aliceClaimable = pool.claimableRevenue(alice);
        require(aliceClaimable >= 499_999_999 && aliceClaimable <= 500 * 1e6, "alice earned 100% rev");

        // 3. Bob buys $2,000 USDC AFTER revenue injection -> Bob MUST have 0 claimable from past revenue!
        vm.startPrank(bob);
        usdc.approve(poolAddr, 2_000 * 1e6);
        uint256 bobTok = pool.buy(2_000 * 1e6, 1);
        vm.stopPrank();
        require(bobTok > 0, "bob buy");
        require(pool.claimableRevenue(bob) == 0, "bob must not dilute past rev");

        // Alice claims her revenue without selling any tokens (plus 1% swap fee from Bob's buy!)
        uint256 aliceUsdcBefore = usdc.balanceOf(alice);
        vm.prank(alice);
        uint256 claimed = pool.claimRevenue();
        require(usdc.balanceOf(alice) - aliceUsdcBefore == claimed, "claim transfer");
        require(claimed >= 519_000_000, "alice gets $500 rev + $20 swap fee from bob");

        // 4. Both Bob and Alice exit 100% of supply -> contract must NEVER revert or go insolvent!
        vm.prank(bob);
        pool.sell(bobTok, 1);
        vm.prank(alice);
        pool.sell(aliceTok, 1);

        require(tok.totalSupply() == 0, "full exit supply 0");
        require(pool.curveReserve() <= 2, "rounding dust strictly favors pool");
    }

    function testFuzzBuySellNeverInsolvent(uint96 amt1Raw, uint96 amt2Raw, uint16 sellPct) public {
        uint256 amt1 = uint256(amt1Raw) % (5_000 * 1e6) + 1_000; // $0.001..$5000
        uint256 amt2 = uint256(amt2Raw) % (5_000 * 1e6) + 1_000;
        uint256 pct = uint256(sellPct) % 100 + 1;

        (address poolAddr, address tokAddr) = factory.createCurve(
            "Fuzz Curve",
            "FUZZ",
            address(usdc),
            5_000 * 1e6,
            100_000_000 * 1e18,
            0, // 0% fee pure invariant check
            address(0),
            ""
        );
        FairCurvePool pool = FairCurvePool(poolAddr);
        FairCurveToken tok = FairCurveToken(tokAddr);

        vm.startPrank(alice);
        usdc.approve(poolAddr, amt1);
        uint256 s1 = pool.buy(amt1, 1);
        vm.stopPrank();

        vm.startPrank(bob);
        usdc.approve(poolAddr, amt2);
        require(pool.buy(amt2, 1) > 0, "bob fuzz buy");
        vm.stopPrank();

        // Check invariant: (V + R) * (Y0 - S) >= V * Y0
        uint256 lhs = (pool.virtualReserve() + pool.curveReserve()) * (pool.virtualSupply() - tok.totalSupply());
        uint256 rhs = pool.virtualReserve() * pool.virtualSupply();
        require(lhs >= rhs, "invariant broken after buys");

        uint256 sell1 = (s1 * pct) / 100;
        if (sell1 > 0) {
            vm.prank(alice);
            pool.sell(sell1, 0);
        }

        // Full remaining exit MUST succeed without insolvency
        uint256 remBob = tok.balanceOf(bob);
        if (remBob > 0) {
            vm.prank(bob);
            pool.sell(remBob, 0);
        }
        uint256 remAlice = tok.balanceOf(alice);
        if (remAlice > 0) {
            vm.prank(alice);
            pool.sell(remAlice, 0);
        }

        require(tok.totalSupply() == 0, "supply not zero");
        require(usdc.balanceOf(poolAddr) >= pool.curveReserve(), "backing shortfall");
    }
}
