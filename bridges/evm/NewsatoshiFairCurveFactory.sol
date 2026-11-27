// SPDX-License-Identifier: LicenseRef-PolyForm-Noncommercial-1.0.0
pragma solidity ^0.8.24;

/// @title NewsatoshiFairCurveFactory — Ownerless, Zero-Premine, PayWords-Revenue-Backed Fair Bonding Curve Launchpad
/// @notice Deploys immutable ERC-20 bonding-curve pools denominated in USDC (or any standard ERC-20 stablecoin)
///         with:
///         - 0% platform extraction, 0% premine, zero owner/admin keys, zero LP rug vector.
///         - Exact integer constant-product math `(V + R) * (Y0 - S) >= V * Y0` where integer floor rounding
///           strictly favors pool solvency (every circulating token is 100% redeemable along the curve at any time).
///         - Dilution-proof `injectRevenue(amount)` hook (`accRevPerToken` 1e18 accumulator) so an AI agent or API
///           org can route settled `NewsatoshiPayWordChannel` / x402 USDC earnings directly to existing token holders,
///           continuously backing the token with real cashflow without minting a single extra token.
interface IERC20Curve {
    function balanceOf(address account) external view returns (uint256);
    function transfer(address to, uint256 value) external returns (bool);
    function transferFrom(address from, address to, uint256 value) external returns (bool);
}

interface IFairCurvePoolHook {
    function onTokenTransfer(address from, address to) external;
}

/// @notice Minimal, immutable ERC-20 whose ONLY minter/burner is its own `FairCurvePool`.
contract FairCurveToken {
    string public name;
    string public symbol;
    uint8 public constant decimals = 18;
    address public immutable pool;

    uint256 public totalSupply;
    mapping(address => uint256) public balanceOf;
    mapping(address => mapping(address => uint256)) public allowance;

    event Transfer(address indexed from, address indexed to, uint256 value);
    event Approval(address indexed owner, address indexed spender, uint256 value);

    modifier onlyPool() {
        require(msg.sender == pool, "ONLY_POOL");
        _;
    }

    constructor(string memory name_, string memory symbol_, address pool_) {
        name = name_;
        symbol = symbol_;
        pool = pool_;
    }

    function approve(address spender, uint256 value) external returns (bool) {
        allowance[msg.sender][spender] = value;
        emit Approval(msg.sender, spender, value);
        return true;
    }

    function transfer(address to, uint256 value) external returns (bool) {
        _move(msg.sender, to, value);
        return true;
    }

    function transferFrom(address from, address to, uint256 value) external returns (bool) {
        uint256 allowed = allowance[from][msg.sender];
        if (allowed != type(uint256).max) {
            require(allowed >= value, "INSUFFICIENT_ALLOWANCE");
            unchecked {
                allowance[from][msg.sender] = allowed - value;
            }
        }
        _move(from, to, value);
        return true;
    }

    function mint(address to, uint256 value) external onlyPool {
        require(to != address(0), "ZERO_TO");
        IFairCurvePoolHook(pool).onTokenTransfer(address(0), to);
        totalSupply += value;
        balanceOf[to] += value;
        emit Transfer(address(0), to, value);
    }

    function burn(address from, uint256 value) external onlyPool {
        require(balanceOf[from] >= value, "INSUFFICIENT_BALANCE");
        IFairCurvePoolHook(pool).onTokenTransfer(from, address(0));
        unchecked {
            balanceOf[from] -= value;
            totalSupply -= value;
        }
        emit Transfer(from, address(0), value);
    }

    function _move(address from, address to, uint256 value) internal {
        require(to != address(0), "ZERO_TO");
        require(balanceOf[from] >= value, "INSUFFICIENT_BALANCE");
        IFairCurvePoolHook(pool).onTokenTransfer(from, to);
        unchecked {
            balanceOf[from] -= value;
        }
        balanceOf[to] += value;
        emit Transfer(from, to, value);
    }
}

/// @notice Immutable constant-product fair bonding curve + dilution-proof x402/PayWords USDC revenue distributor.
contract FairCurvePool is IFairCurvePoolHook {
    uint256 public constant BPS = 10_000;
    uint256 public constant MAX_SWAP_FEE_BPS = 500; // <= 5% max, 0% platform extraction
    uint256 public constant REV_PRECISION = 1e30;

    address public immutable factory;
    address public immutable reserveToken;
    FairCurveToken public immutable token;
    address public immutable feeSink; // address(0) => swap fees feed holder revenue directly!
    uint16 public immutable swapFeeBps;
    uint256 public immutable virtualReserve;
    uint256 public immutable virtualSupply;
    string public metadataUri;

    /// @notice Real reserve backing the constant-product curve (`curveReserve` is ALWAYS >= exact sell liability).
    uint256 public curveReserve;

    /// @notice Dilution-proof revenue dividend accumulator (scaled by `1e18` per 1e18 token wei).
    uint256 public accRevPerToken;
    uint256 public totalRevenueInjected;
    uint256 public totalRevenueClaimed;
    mapping(address => uint256) public userRevPaid;
    mapping(address => uint256) public pendingRevenue;

    uint256 private _locked = 1;

    event CurveBuy(
        address indexed buyer,
        uint256 reserveIn,
        uint256 feeAmount,
        uint256 tokensOut,
        uint256 newCurveReserve,
        uint256 newTotalSupply
    );

    event CurveSell(
        address indexed seller,
        uint256 tokensIn,
        uint256 grossReserveOut,
        uint256 feeAmount,
        uint256 netReserveOut,
        uint256 newCurveReserve,
        uint256 newTotalSupply
    );

    event RevenueInjected(
        address indexed payer,
        uint256 amount,
        uint256 newAccRevPerToken,
        uint256 activeSupply
    );

    event RevenueClaimed(address indexed holder, uint256 amount);

    modifier nonReentrant() {
        require(_locked == 1, "REENTRANCY");
        _locked = 2;
        _;
        _locked = 1;
    }

    constructor(
        string memory name_,
        string memory symbol_,
        address reserveToken_,
        uint256 virtualReserve_,
        uint256 virtualSupply_,
        uint16 swapFeeBps_,
        address feeSink_,
        string memory metadataUri_
    ) {
        require(reserveToken_ != address(0), "INVALID_RESERVE_TOKEN");
        require(virtualReserve_ >= 1e4 && virtualReserve_ <= 1e18, "INVALID_VIRTUAL_RESERVE");
        require(virtualSupply_ >= 1e18 && virtualSupply_ <= 1e30, "INVALID_VIRTUAL_SUPPLY");
        require(swapFeeBps_ <= MAX_SWAP_FEE_BPS, "FEE_TOO_HIGH");

        factory = msg.sender;
        reserveToken = reserveToken_;
        virtualReserve = virtualReserve_;
        virtualSupply = virtualSupply_;
        swapFeeBps = swapFeeBps_;
        feeSink = feeSink_;
        metadataUri = metadataUri_;
        token = new FairCurveToken(name_, symbol_, address(this));
    }

    /// @notice Exact integer constant-product buy quote (floor rounding strictly favors pool reserve).
    function quoteBuy(uint256 reserveIn) public view returns (uint256 tokensOut, uint256 feeAmount, uint256 netIn) {
        if (reserveIn == 0) return (0, 0, 0);
        uint256 supply = token.totalSupply();
        feeAmount = (feeSink == address(0) && supply == 0) ? 0 : (reserveIn * uint256(swapFeeBps)) / BPS;
        netIn = reserveIn - feeAmount;
        if (netIn == 0) return (0, feeAmount, 0);
        uint256 x = virtualReserve + curveReserve;
        uint256 y = virtualSupply - supply;
        tokensOut = (y * netIn) / (x + netIn);
    }

    /// @notice Exact integer constant-product sell quote (floor rounding strictly guarantees `grossOut <= curveReserve`).
    function quoteSell(uint256 tokensIn) public view returns (uint256 netOut, uint256 feeAmount, uint256 grossOut) {
        uint256 supply = token.totalSupply();
        if (tokensIn == 0 || supply == 0 || tokensIn > supply) return (0, 0, 0);
        uint256 x = virtualReserve + curveReserve;
        uint256 y = virtualSupply - supply;
        grossOut = (x * tokensIn) / (y + tokensIn);
        feeAmount = (grossOut * uint256(swapFeeBps)) / BPS;
        netOut = grossOut - feeAmount;
    }

    /// @notice Current marginal spot price of 1 full token (`1e18` units) denominated in reserve token units.
    function spotPrice() external view returns (uint256) {
        uint256 x = virtualReserve + curveReserve;
        uint256 y = virtualSupply - token.totalSupply();
        return (x * 1e18) / y;
    }

    /// @notice View pending claimable revenue dividends for `account` in reserve token units.
    function claimableRevenue(address account) public view returns (uint256) {
        uint256 bal = token.balanceOf(account);
        uint256 accrued = (bal * (accRevPerToken - userRevPaid[account])) / REV_PRECISION;
        return pendingRevenue[account] + accrued;
    }

    /// @notice Buy tokens directly from the fair curve using `reserveToken` (e.g. Base USDC).
    function buy(uint256 reserveIn, uint256 minTokensOut) external nonReentrant returns (uint256 tokensOut) {
        require(reserveIn > 0, "AMOUNT_ZERO");
        (uint256 out, uint256 fee, uint256 netIn) = quoteBuy(reserveIn);
        require(out > 0 && out >= minTokensOut, "SLIPPAGE_OR_DUST");

        _pullExact(msg.sender, reserveIn);

        uint256 supplyBefore = token.totalSupply();
        if (fee > 0) {
            if (feeSink != address(0)) {
                _safeTransfer(reserveToken, feeSink, fee);
            } else if (supplyBefore > 0) {
                // Route 100% of swap fee strictly to pre-existing holders before minting buyer's tokens.
                _creditRevenue(fee, supplyBefore);
            } else {
                // First buy with feeSink==0: keep fee inside curveReserve so no funds are ever stranded.
                curveReserve += fee;
            }
        }
        curveReserve += netIn;
        token.mint(msg.sender, out);

        emit CurveBuy(msg.sender, reserveIn, fee, out, curveReserve, token.totalSupply());
        return out;
    }

    /// @notice Sell tokens back into the fair curve at any time for guaranteed reserve liquidity.
    function sell(uint256 tokensIn, uint256 minReserveOut) external nonReentrant returns (uint256 netOut) {
        require(tokensIn > 0, "AMOUNT_ZERO");
        (uint256 net, uint256 fee, uint256 gross) = quoteSell(tokensIn);
        require(gross > 0 && gross <= curveReserve, "INSUFFICIENT_LIQUIDITY");
        require(net >= minReserveOut, "SLIPPAGE");

        token.burn(msg.sender, tokensIn);
        curveReserve -= gross;

        if (fee > 0) {
            if (feeSink != address(0)) {
                _safeTransfer(reserveToken, feeSink, fee);
            } else if (token.totalSupply() > 0) {
                _creditRevenue(fee, token.totalSupply());
            } else {
                // Final exit with feeSink==0: pay full gross back to last holder rather than trapping dust.
                net += fee;
                fee = 0;
            }
        }

        _safeTransfer(reserveToken, msg.sender, net);
        emit CurveSell(msg.sender, tokensIn, gross, fee, net, curveReserve, token.totalSupply());
        return net;
    }

    /// @notice Inject real HTTP 402 / PayWords USDC API earnings directly to existing token holders.
    ///         100% dilution-proof: only tokens circulating BEFORE this call receive these dividends.
    function injectRevenue(uint256 amount) external nonReentrant {
        require(amount > 0, "AMOUNT_ZERO");
        uint256 supply = token.totalSupply();
        require(supply > 0, "NO_CIRCULATING_SUPPLY");

        _pullExact(msg.sender, amount);
        _creditRevenue(amount, supply);
        emit RevenueInjected(msg.sender, amount, accRevPerToken, supply);
    }

    /// @notice Claim all accumulated USDC revenue dividends without selling any tokens.
    function claimRevenue() external nonReentrant returns (uint256 payout) {
        _syncAccount(msg.sender);
        payout = pendingRevenue[msg.sender];
        require(payout > 0, "NO_REVENUE");
        pendingRevenue[msg.sender] = 0;
        totalRevenueClaimed += payout;
        _safeTransfer(reserveToken, msg.sender, payout);
        emit RevenueClaimed(msg.sender, payout);
    }

    /// @notice Called strictly by `FairCurveToken` before every mint/burn/transfer to settle accrued revenue.
    function onTokenTransfer(address from, address to) external override {
        require(msg.sender == address(token), "ONLY_TOKEN");
        if (from != address(0)) _syncAccount(from);
        if (to != address(0)) _syncAccount(to);
    }

    function _syncAccount(address account) internal {
        uint256 bal = token.balanceOf(account);
        if (bal > 0) {
            uint256 delta = accRevPerToken - userRevPaid[account];
            if (delta > 0) {
                pendingRevenue[account] += (bal * delta) / REV_PRECISION;
            }
        }
        userRevPaid[account] = accRevPerToken;
    }

    function _creditRevenue(uint256 amount, uint256 activeSupply) internal {
        accRevPerToken += (amount * REV_PRECISION) / activeSupply;
        totalRevenueInjected += amount;
    }

    function _pullExact(address from, uint256 amount) internal {
        uint256 balBefore = IERC20Curve(reserveToken).balanceOf(address(this));
        _safeTransferFrom(reserveToken, from, address(this), amount);
        uint256 balAfter = IERC20Curve(reserveToken).balanceOf(address(this));
        require(balAfter >= balBefore && balAfter - balBefore == amount, "FEE_ON_TRANSFER_UNSUPPORTED");
    }

    function _safeTransfer(address tok, address to, uint256 amount) internal {
        (bool ok, bytes memory data) = tok.call(
            abi.encodeWithSelector(IERC20Curve.transfer.selector, to, amount)
        );
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "ERC20_TRANSFER_FAILED");
    }

    function _safeTransferFrom(address tok, address from, address to, uint256 amount) internal {
        (bool ok, bytes memory data) = tok.call(
            abi.encodeWithSelector(IERC20Curve.transferFrom.selector, from, to, amount)
        );
        require(ok && (data.length == 0 || abi.decode(data, (bool))), "ERC20_TRANSFER_FROM_FAILED");
    }
}

/// @notice Permissionless, ownerless factory deploying verified fair bonding curves in 1 transaction.
contract NewsatoshiFairCurveFactory {
    address[] public allPools;
    mapping(address => bool) public isPool;

    event FairCurveCreated(
        address indexed pool,
        address indexed token,
        address indexed creator,
        address reserveToken,
        uint256 virtualReserve,
        uint256 virtualSupply,
        uint16 swapFeeBps,
        address feeSink
    );

    function createCurve(
        string calldata name_,
        string calldata symbol_,
        address reserveToken_,
        uint256 virtualReserve_,
        uint256 virtualSupply_,
        uint16 swapFeeBps_,
        address feeSink_,
        string calldata metadataUri_
    ) external returns (address pool, address token) {
        FairCurvePool p = new FairCurvePool(
            name_,
            symbol_,
            reserveToken_,
            virtualReserve_,
            virtualSupply_,
            swapFeeBps_,
            feeSink_,
            metadataUri_
        );
        pool = address(p);
        token = address(p.token());
        isPool[pool] = true;
        allPools.push(pool);

        emit FairCurveCreated(
            pool,
            token,
            msg.sender,
            reserveToken_,
            virtualReserve_,
            virtualSupply_,
            swapFeeBps_,
            feeSink_
        );
    }

    function poolCount() external view returns (uint256) {
        return allPools.length;
    }
}
